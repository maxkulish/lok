use crate::workflow::StepResult;
use minijinja::value::{Object, Value};
use std::collections::HashMap;
use std::fmt;

/// Lazy environment variable lookup via MiniJinja's Object trait.
///
/// Reads env vars on demand - never eagerly enumerates the environment.
#[derive(Debug)]
struct LazyEnv;

impl fmt::Display for LazyEnv {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(f, "env")
    }
}

impl Object for LazyEnv {
    fn get_value(self: &std::sync::Arc<Self>, key: &Value) -> Option<Value> {
        let key_str = key.as_str()?;
        std::env::var(key_str).ok().map(Value::from)
    }
}

/// Template context that builds a MiniJinja value tree from workflow state.
///
/// Provides the same variable paths as the regex interpolation system:
/// `steps.{name}.output`, `steps.{name}.{field}`, `steps.{name}.success`,
/// `env.{VAR}`, `arg.{N}` (1-indexed), `workflow.backends`,
/// `item`, `item.{field}`, `index`.
#[allow(dead_code)]
pub struct TemplateContext {
    values: Value,
}

#[allow(dead_code)]
impl TemplateContext {
    /// Build a template context from workflow state.
    ///
    /// - `steps`: completed step results keyed by step name
    /// - `args`: positional arguments stored as a 1-indexed sequence with
    ///   `UNDEFINED` at index 0, so `{{ arg.1 }}`, `{{ arg.2 }}` resolve via
    ///   sequence indexing
    /// - `backends`: backend names used (will be capitalized, deduplicated, joined)
    pub fn new(steps: &HashMap<String, StepResult>, args: &[String], backends: &[String]) -> Self {
        let mut root = std::collections::BTreeMap::new();

        // Build steps namespace
        let mut steps_map = std::collections::BTreeMap::new();
        for (name, result) in steps {
            let mut step_map = std::collections::BTreeMap::new();
            step_map.insert("output".to_string(), Value::from(result.output.clone()));
            step_map.insert("success".to_string(), Value::from(result.success));

            // Add parsed fields from parsed_output or fallback to string parsing
            // Extract JSON fields from parsed_output or fallback to extracting JSON from
            // markdown-fenced blocks in the raw output (matches legacy extract_json_field).
            let json_source = if result.parsed_output.is_some() {
                result.parsed_output.clone()
            } else {
                crate::workflow::extract_json_from_text(&result.output).and_then(|s| {
                    serde_json::from_str::<serde_json::Value>(&s)
                        .or_else(|_| {
                            let sanitized = crate::workflow::sanitize_json_strings(&s);
                            serde_json::from_str::<serde_json::Value>(&sanitized)
                        })
                        .ok()
                })
            };
            if let Some(ref parsed) = json_source {
                if let Some(obj) = parsed.as_object() {
                    for (k, v) in obj {
                        if k != "output" && k != "success" {
                            step_map.insert(k.to_string(), Value::from_serialize(v));
                        }
                    }
                }
            }

            steps_map.insert(name.clone(), Value::from_serialize(&step_map));
        }
        root.insert("steps".to_string(), Value::from_serialize(&steps_map));

        // Build env namespace (lazy)
        root.insert("env".to_string(), Value::from_object(LazyEnv));

        // Build arg namespace as a sequence (1-indexed via index 0 placeholder)
        // {{ arg.1 }} accesses the first argument via sequence indexing
        let mut arg_seq: Vec<Value> = vec![Value::UNDEFINED]; // index 0 placeholder
        for arg in args {
            arg_seq.push(Value::from(arg.clone()));
        }
        root.insert("arg".to_string(), Value::from_serialize(&arg_seq));

        // Build workflow namespace
        let mut workflow_map = std::collections::BTreeMap::new();
        let mut unique_backends: Vec<String> = backends.to_vec();
        unique_backends.sort();
        unique_backends.dedup();
        let formatted: Vec<String> = unique_backends
            .iter()
            .map(|b| {
                let mut chars = b.chars();
                match chars.next() {
                    Some(c) => c.to_uppercase().to_string() + chars.as_str(),
                    None => String::new(),
                }
            })
            .collect();
        let backends_str = if formatted.is_empty() {
            "lok".to_string()
        } else {
            formatted.join(" + ")
        };
        workflow_map.insert("backends".to_string(), Value::from(backends_str));
        root.insert("workflow".to_string(), Value::from_serialize(&workflow_map));

        Self {
            values: Value::from_serialize(&root),
        }
    }

    /// Add loop variables to the context for for_each iteration.
    ///
    /// Returns a new context with `item`, `item.{field}`, and `index` available.
    pub fn with_loop_item(self, item: Value, index: usize) -> Self {
        let mut root = std::collections::BTreeMap::<String, Value>::new();

        // Copy existing top-level keys by iterating the value
        if let Ok(iter) = self.values.try_iter() {
            for key in iter {
                if let Some(key_str) = key.as_str() {
                    if let Ok(val) = self.values.get_attr(key_str) {
                        root.insert(key_str.to_string(), val);
                    }
                }
            }
        }

        root.insert("item".to_string(), item);
        root.insert("index".to_string(), Value::from(index));

        Self {
            values: Value::from_serialize(&root),
        }
    }

    /// Get the underlying MiniJinja value for rendering.
    pub fn as_value(&self) -> &Value {
        &self.values
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::StepResult;

    fn make_step(name: &str, output: &str, success: bool) -> StepResult {
        StepResult {
            name: name.to_string(),
            output: output.to_string(),
            parsed_output: None,
            success,
            elapsed_ms: 100,
            backend: Some("test".to_string()),
            raw_output: None,
            stderr: None,
            exit_code: None,
            validation: None,
            failure: None,
        }
    }

    fn make_step_with_parsed(
        name: &str,
        output: &str,
        parsed: serde_json::Value,
        success: bool,
    ) -> StepResult {
        StepResult {
            name: name.to_string(),
            output: output.to_string(),
            parsed_output: Some(parsed),
            success,
            elapsed_ms: 100,
            backend: Some("test".to_string()),
            raw_output: None,
            stderr: None,
            exit_code: None,
            validation: None,
            failure: None,
        }
    }

    fn render_template(template: &str, ctx: &TemplateContext) -> String {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        let tmpl = env.template_from_str(template).unwrap();
        tmpl.render(ctx.as_value()).unwrap()
    }

    #[test]
    fn test_step_output() {
        let mut steps = HashMap::new();
        steps.insert("fetch".to_string(), make_step("fetch", "hello world", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert_eq!(
            render_template("{{ steps.fetch.output }}", &ctx),
            "hello world"
        );
    }

    #[test]
    fn test_step_field_with_parsed_output() {
        let mut steps = HashMap::new();
        let parsed = serde_json::json!({"verdict": "approve", "score": 95});
        steps.insert(
            "review".to_string(),
            make_step_with_parsed("review", "{}", parsed, true),
        );
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert_eq!(
            render_template("{{ steps.review.verdict }}", &ctx),
            "approve"
        );
        assert_eq!(render_template("{{ steps.review.score }}", &ctx), "95");
    }

    #[test]
    fn test_step_field_fallback_no_parsed_output() {
        let mut steps = HashMap::new();
        steps.insert(
            "fetch".to_string(),
            make_step("fetch", r#"{"verdict": "pass", "count": 3}"#, true),
        );
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert_eq!(render_template("{{ steps.fetch.verdict }}", &ctx), "pass");
    }

    #[test]
    fn test_step_success_true() {
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "ok", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert_eq!(render_template("{{ steps.s.success }}", &ctx), "true");
    }

    #[test]
    fn test_step_success_false() {
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "fail", false));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert_eq!(render_template("{{ steps.s.success }}", &ctx), "false");
    }

    #[test]
    fn test_env_lookup() {
        std::env::set_var("LOK_TEST_CTX_VAR", "found_it");
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        assert_eq!(
            render_template("{{ env.LOK_TEST_CTX_VAR }}", &ctx),
            "found_it"
        );
        std::env::remove_var("LOK_TEST_CTX_VAR");
    }

    #[test]
    fn test_env_missing() {
        std::env::remove_var("LOK_TEST_NONEXISTENT_VAR_12345");
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        let tmpl = env
            .template_from_str("{{ env.LOK_TEST_NONEXISTENT_VAR_12345 }}")
            .unwrap();
        let result = tmpl.render(ctx.as_value());
        assert!(result.is_err());
    }

    #[test]
    fn test_arg_access() {
        let ctx = TemplateContext::new(
            &HashMap::new(),
            &["first".to_string(), "second".to_string()],
            &[],
        );
        assert_eq!(render_template("{{ arg.1 }}", &ctx), "first");
        assert_eq!(render_template("{{ arg.2 }}", &ctx), "second");
    }

    #[test]
    fn test_arg_zero_undefined() {
        // arg.0 is undefined (args are 1-indexed, index 0 is an undefined placeholder)
        let ctx = TemplateContext::new(&HashMap::new(), &["val".to_string()], &[]);
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        let tmpl = env.template_from_str("{{ arg.0 }}").unwrap();
        let result = tmpl.render(ctx.as_value());
        assert!(result.is_err());
    }

    #[test]
    fn test_arg_out_of_bounds() {
        let ctx = TemplateContext::new(&HashMap::new(), &["val".to_string()], &[]);
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(minijinja::UndefinedBehavior::Strict);
        let tmpl = env.template_from_str("{{ arg.5 }}").unwrap();
        let result = tmpl.render(ctx.as_value());
        assert!(result.is_err());
    }

    #[test]
    fn test_workflow_backends() {
        let ctx = TemplateContext::new(
            &HashMap::new(),
            &[],
            &[
                "claude".to_string(),
                "gemini".to_string(),
                "claude".to_string(),
            ],
        );
        assert_eq!(
            render_template("{{ workflow.backends }}", &ctx),
            "Claude + Gemini"
        );
    }

    #[test]
    fn test_workflow_backends_empty() {
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        assert_eq!(render_template("{{ workflow.backends }}", &ctx), "lok");
    }

    #[test]
    fn test_loop_vars_string_item() {
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "out", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        let ctx = ctx.with_loop_item(Value::from("hello"), 0);
        assert_eq!(render_template("{{ item }}-{{ index }}", &ctx), "hello-0");
    }

    #[test]
    fn test_loop_vars_object_item() {
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        let mut item_map = std::collections::BTreeMap::new();
        item_map.insert("name".to_string(), Value::from("test"));
        item_map.insert("value".to_string(), Value::from(42));
        let ctx = ctx.with_loop_item(Value::from_serialize(&item_map), 3);
        assert_eq!(render_template("{{ item.name }}", &ctx), "test");
        assert_eq!(render_template("{{ item.value }}", &ctx), "42");
        assert_eq!(render_template("{{ index }}", &ctx), "3");
    }

    #[test]
    fn test_loop_vars_preserve_existing_namespaces() {
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "out", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        let ctx = ctx.with_loop_item(Value::from("hello"), 0);
        assert_eq!(render_template("{{ item }}", &ctx), "hello");
        assert_eq!(render_template("{{ index }}", &ctx), "0");
        assert_eq!(render_template("{{ steps.s.output }}", &ctx), "out");
    }
}
