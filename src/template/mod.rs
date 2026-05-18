mod context;
mod filters;

pub use context::TemplateContext;

use minijinja::UndefinedBehavior;

/// Errors that can occur during template rendering.
#[derive(Debug, thiserror::Error)]
pub enum TemplateError {
    /// A referenced variable was not found in the template context.
    #[error("undefined variable: {0}")]
    UndefinedVariable(#[source] minijinja::Error),

    /// The template string could not be parsed.
    #[error("template parse error: {0}")]
    ParseError(#[source] minijinja::Error),

    /// An error occurred while rendering the template.
    #[error("render error: {0}")]
    RenderError(#[source] minijinja::Error),
}

impl TemplateError {
    fn from_minijinja(err: minijinja::Error) -> Self {
        match err.kind() {
            minijinja::ErrorKind::UndefinedError => TemplateError::UndefinedVariable(err),
            minijinja::ErrorKind::SyntaxError | minijinja::ErrorKind::InvalidOperation
                if err.line().is_some() =>
            {
                TemplateError::ParseError(err)
            }
            _ => TemplateError::RenderError(err),
        }
    }

    /// Byte range in the original template source where the error occurred.
    ///
    /// Returns the span of the failing expression (e.g. `steps.missing.output` for
    /// an undefined-variable error) so callers can extract the exact offending token
    /// instead of guessing it from the template. Returns `None` if MiniJinja could
    /// not associate the error with a source span.
    pub fn source_range(&self) -> Option<std::ops::Range<usize>> {
        let inner = match self {
            TemplateError::UndefinedVariable(e)
            | TemplateError::ParseError(e)
            | TemplateError::RenderError(e) => e,
        };
        inner.range()
    }
}

/// Template engine backed by MiniJinja 2.
///
/// Stateless after construction - create once and reuse across calls.
/// Registers custom filters on construction and uses strict undefined behavior.
#[allow(dead_code)]
pub struct TemplateEngine {
    env: minijinja::Environment<'static>,
}

#[allow(dead_code)]
impl TemplateEngine {
    /// Create a new template engine with custom filters registered.
    ///
    /// Uses [`UndefinedBehavior::SemiStrict`] so the `default()` filter and `is defined`
    /// test can intercept missing values, while rendering an undefined value as the final
    /// output still errors - preserving the strict-undefined contract for
    /// `WorkflowError::UnknownVariable` reporting.
    pub fn new() -> Self {
        let mut env = minijinja::Environment::new();
        env.set_undefined_behavior(UndefinedBehavior::SemiStrict);
        filters::register_filters(&mut env);
        Self { env }
    }

    /// Render a template string with the given context.
    pub fn render(&self, template: &str, ctx: &TemplateContext) -> Result<String, TemplateError> {
        let tmpl = self
            .env
            .template_from_str(template)
            .map_err(TemplateError::from_minijinja)?;
        tmpl.render(ctx.as_value())
            .map_err(TemplateError::from_minijinja)
    }

    /// Evaluate a Jinja expression string against the context and coerce to bool.
    ///
    /// Used for step `when` conditions. Returns the truthiness of the evaluated
    /// expression value. Undefined variables produce `TemplateError::UndefinedVariable`.
    pub fn eval_expression(
        &self,
        expr: &str,
        ctx: &TemplateContext,
    ) -> Result<bool, TemplateError> {
        let compiled = self
            .env
            .compile_expression(expr)
            .map_err(TemplateError::from_minijinja)?;
        let result = compiled
            .eval(ctx.as_value())
            .map_err(TemplateError::from_minijinja)?;
        Ok(result.is_true())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::workflow::StepResult;
    use std::collections::HashMap;

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
            usage: None,
        }
    }

    #[test]
    fn test_render_mixed() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("x".to_string(), make_step("x", "  hello world  ", true));
        let ctx = TemplateContext::new(&steps, &[], &["claude".to_string()]);
        let result = engine
            .render(r#"{{ steps.x.output | trim | default_val("none") }}"#, &ctx)
            .unwrap();
        assert_eq!(result, "hello world");
    }

    #[test]
    fn test_no_reexpansion_of_braces_in_output() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert(
            "x".to_string(),
            make_step("x", "value is {{ secret }}", true),
        );
        let ctx = TemplateContext::new(&steps, &[], &[]);
        let result = engine.render("{{ steps.x.output }}", &ctx).unwrap();
        assert_eq!(result, "value is {{ secret }}");
    }

    #[test]
    fn test_combined_env_arg_step() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "out", true));
        std::env::set_var("LOK_TEST_TMPL_VAR", "envval");
        let ctx = TemplateContext::new(&steps, &["argval".to_string()], &[]);
        let result = engine
            .render(
                "{{ steps.s.output }}-{{ env.LOK_TEST_TMPL_VAR }}-{{ arg.1 }}",
                &ctx,
            )
            .unwrap();
        std::env::remove_var("LOK_TEST_TMPL_VAR");
        assert_eq!(result, "out-envval-argval");
    }

    #[test]
    fn test_parse_error() {
        let engine = TemplateEngine::new();
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        let err = engine.render("{{ steps.x", &ctx).unwrap_err();
        assert!(matches!(err, TemplateError::ParseError(_)));
    }

    #[test]
    fn test_undefined_variable() {
        let engine = TemplateEngine::new();
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        let err = engine
            .render("{{ steps.nonexistent.output }}", &ctx)
            .unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }

    #[test]
    fn test_eval_expression_truthy() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "PASS", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert!(engine
            .eval_expression(r#"steps.s.success and "PASS" in steps.s.output"#, &ctx)
            .unwrap());
    }

    #[test]
    fn test_eval_expression_falsy() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("s".to_string(), make_step("s", "FAIL", false));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        assert!(!engine.eval_expression("steps.s.success", &ctx).unwrap());
    }

    #[test]
    fn test_eval_expression_undefined() {
        let engine = TemplateEngine::new();
        let ctx = TemplateContext::new(&HashMap::new(), &[], &[]);
        let err = engine
            .eval_expression("steps.missing.success", &ctx)
            .unwrap_err();
        assert!(matches!(err, TemplateError::UndefinedVariable(_)));
    }
}
