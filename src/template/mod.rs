mod context;
mod filters;

pub use context::TemplateContext;

use minijinja::syntax::SyntaxConfig;
use minijinja::UndefinedBehavior;

/// Comment start delimiter that no workflow text is expected to contain.
///
/// MiniJinja's default comment opener `{#` is ordinary text in the fields lok renders:
/// shell uses `${#VAR}` for string length, and markdown uses `{#id}` for heading
/// attributes. MiniJinja rejects an empty comment delimiter, so comments are disabled
/// by moving the delimiters to this placeholder. `{#` and `#}` are then plain text,
/// and anything between them is rendered like any other template text.
const COMMENT_START: &str = "{#lok-comments-disabled";

/// Comment end delimiter paired with [`COMMENT_START`].
const COMMENT_END: &str = "lok-comments-disabled#}";

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
    /// Classify a MiniJinja error.
    ///
    /// Only `SyntaxError` is a parse error. Render-time failures such as
    /// `InvalidOperation` also carry a line number, so the line alone does not
    /// distinguish them from syntax errors.
    fn from_minijinja(err: minijinja::Error) -> Self {
        match err.kind() {
            minijinja::ErrorKind::UndefinedError => TemplateError::UndefinedVariable(err),
            minijinja::ErrorKind::SyntaxError if err.line().is_some() => {
                TemplateError::ParseError(err)
            }
            _ => TemplateError::RenderError(err),
        }
    }

    fn inner(&self) -> &minijinja::Error {
        match self {
            TemplateError::UndefinedVariable(e)
            | TemplateError::ParseError(e)
            | TemplateError::RenderError(e) => e,
        }
    }

    /// Byte range of the failing expression in the rendered template source.
    ///
    /// This is a location, not a variable name. For `{{ steps.missing.output }}` it
    /// covers `.missing.output`, and for `{{ steps.x.absent | upper }}` it covers
    /// `upper`. For a syntax error at the end of input it can point past the end of
    /// the source. Returns `None` if MiniJinja could not associate the error with a
    /// source span.
    pub fn source_range(&self) -> Option<std::ops::Range<usize>> {
        self.inner().range()
    }

    /// 1-based line in the template source where the error occurred, if known.
    pub fn line(&self) -> Option<usize> {
        self.inner().line()
    }

    /// MiniJinja's error kind and detail, such as `syntax error: unknown statement frob`,
    /// without the `(in <string>:N)` location suffix of its `Display` form.
    pub fn message(&self) -> String {
        let inner = self.inner();
        match inner.detail() {
            Some(detail) => format!("{}: {}", inner.kind(), detail),
            None => inner.kind().to_string(),
        }
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
    ///
    /// Jinja comment syntax is disabled; see [`COMMENT_START`].
    pub fn new() -> Self {
        let mut env = minijinja::Environment::new();
        env.set_syntax(
            SyntaxConfig::builder()
                .comment_delimiters(COMMENT_START, COMMENT_END)
                .build()
                .expect("comment delimiters are distinct from the variable and block delimiters"),
        );
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
    fn test_comment_openers_render_verbatim() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("x".to_string(), make_step("x", "out", true));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        let template =
            "[ ${#OUTPUT} -lt 100 ] ${#} ${##}\n## Setup {#setup}\n{# note #} {{ steps.x.output }}";
        let result = engine.render(template, &ctx).unwrap();
        assert_eq!(
            result,
            "[ ${#OUTPUT} -lt 100 ] ${#} ${##}\n## Setup {#setup}\n{# note #} out"
        );
    }

    #[test]
    fn test_if_else_blocks_render() {
        let engine = TemplateEngine::new();
        let mut steps = HashMap::new();
        steps.insert("x".to_string(), make_step("x", "out", false));
        let ctx = TemplateContext::new(&steps, &[], &[]);
        let template = "{% if steps.x.success %}yes{% else %}no{% endif %}";
        assert_eq!(engine.render(template, &ctx).unwrap(), "no");
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
