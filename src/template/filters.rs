use minijinja::value::Value;
use minijinja::Environment;

/// Register all custom filters on a MiniJinja environment.
pub fn register_filters(env: &mut Environment) {
    env.add_filter("shell_escape", shell_escape);
    env.add_filter("json_encode", json_encode);
    env.add_filter("join", join_filter);
    env.add_filter("first", first);
    env.add_filter("last", last);
    env.add_filter("default_val", default_val);
    env.add_filter("trim", trim);
    env.add_filter("lines", lines);
}

/// Escape a string for safe use in shell commands.
///
/// Wraps the value in single quotes and escapes embedded single quotes
/// using the `'\''` pattern. Strips null bytes.
fn shell_escape(value: &str) -> String {
    let sanitized = value.replace('\0', "");
    let escaped = sanitized.replace('\'', "'\\''");
    format!("'{}'", escaped)
}

/// Serialize a value to a JSON string.
fn json_encode(value: Value) -> Result<String, minijinja::Error> {
    let serialized = serde_json::to_string(&value).map_err(|e| {
        minijinja::Error::new(
            minijinja::ErrorKind::InvalidOperation,
            format!("json_encode failed: {}", e),
        )
    })?;
    Ok(serialized)
}

/// Join a sequence of values with a separator string.
///
/// Default separator is an empty string if not provided.
fn join_filter(value: Value, separator: Option<&str>) -> Result<String, minijinja::Error> {
    let sep = separator.unwrap_or("");
    let mut parts = Vec::new();
    for item in value.try_iter()? {
        parts.push(item.to_string());
    }
    Ok(parts.join(sep))
}

/// Return the first element of a sequence, or undefined if empty.
fn first(value: Value) -> Result<Value, minijinja::Error> {
    Ok(value.try_iter()?.next().unwrap_or(Value::UNDEFINED))
}

/// Return the last element of a sequence, or undefined if empty.
fn last(value: Value) -> Result<Value, minijinja::Error> {
    Ok(value.try_iter()?.last().unwrap_or(Value::UNDEFINED))
}

/// Return the value if it is defined and truthy, otherwise return the fallback.
fn default_val(value: Value, fallback: Value) -> Value {
    if value.is_undefined() || value.is_none() {
        return fallback;
    }
    if let Some(s) = value.as_str() {
        if s.is_empty() {
            return fallback;
        }
    }
    value
}

/// Strip leading and trailing whitespace from a string value.
fn trim(value: &str) -> String {
    value.trim().to_string()
}

/// Split a string into a sequence of lines.
fn lines(value: &str) -> Vec<String> {
    value.lines().map(|l| l.to_string()).collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use minijinja::value::Value;

    #[test]
    fn test_shell_escape_basic() {
        assert_eq!(shell_escape("hello"), "'hello'");
    }

    #[test]
    fn test_shell_escape_single_quotes() {
        assert_eq!(shell_escape("it's"), "'it'\\''s'");
    }

    #[test]
    fn test_shell_escape_injection() {
        let input = "'; rm -rf /; echo '";
        let result = shell_escape(input);
        assert_eq!(result, "''\\''; rm -rf /; echo '\\'''");
    }

    #[test]
    fn test_shell_escape_backticks_and_dollar() {
        let input = "$(whoami) `id`";
        let result = shell_escape(input);
        assert_eq!(result, "'$(whoami) `id`'");
    }

    #[test]
    fn test_shell_escape_newlines() {
        let input = "line1\nline2";
        let result = shell_escape(input);
        assert_eq!(result, "'line1\nline2'");
    }

    #[test]
    fn test_shell_escape_null_bytes() {
        let input = "before\0after";
        let result = shell_escape(input);
        assert_eq!(result, "'beforeafter'");
    }

    #[test]
    fn test_shell_escape_unicode() {
        let input = "hello world";
        let result = shell_escape(input);
        assert_eq!(result, "'hello world'");
    }

    #[test]
    fn test_json_encode_string() {
        let result = json_encode(Value::from("hello")).unwrap();
        assert_eq!(result, "\"hello\"");
    }

    #[test]
    fn test_json_encode_number() {
        let result = json_encode(Value::from(42)).unwrap();
        assert_eq!(result, "42");
    }

    #[test]
    fn test_json_encode_nested() {
        let mut map = std::collections::BTreeMap::new();
        map.insert("key".to_string(), Value::from("value"));
        let result = json_encode(Value::from_serialize(&map)).unwrap();
        assert_eq!(result, r#"{"key":"value"}"#);
    }

    #[test]
    fn test_join_with_separator() {
        let seq = Value::from(vec!["a", "b", "c"]);
        let result = join_filter(seq, Some(", ")).unwrap();
        assert_eq!(result, "a, b, c");
    }

    #[test]
    fn test_join_default_separator() {
        let seq = Value::from(vec!["a", "b"]);
        let result = join_filter(seq, None).unwrap();
        assert_eq!(result, "ab");
    }

    #[test]
    fn test_join_empty() {
        let seq = Value::from(Vec::<String>::new());
        let result = join_filter(seq, Some(",")).unwrap();
        assert_eq!(result, "");
    }

    #[test]
    fn test_first_normal() {
        let seq = Value::from(vec!["a", "b", "c"]);
        let result = first(seq).unwrap();
        assert_eq!(result.to_string(), "a");
    }

    #[test]
    fn test_first_empty() {
        let seq = Value::from(Vec::<String>::new());
        assert!(first(seq).unwrap().is_undefined());
    }

    #[test]
    fn test_first_single() {
        let seq = Value::from(vec!["only"]);
        let result = first(seq).unwrap();
        assert_eq!(result.to_string(), "only");
    }

    #[test]
    fn test_last_normal() {
        let seq = Value::from(vec!["a", "b", "c"]);
        let result = last(seq).unwrap();
        assert_eq!(result.to_string(), "c");
    }

    #[test]
    fn test_last_empty() {
        let seq = Value::from(Vec::<String>::new());
        assert!(last(seq).unwrap().is_undefined());
    }

    #[test]
    fn test_last_single() {
        let seq = Value::from(vec!["only"]);
        let result = last(seq).unwrap();
        assert_eq!(result.to_string(), "only");
    }

    #[test]
    fn test_default_val_defined() {
        let result = default_val(Value::from("hello"), Value::from("fallback"));
        assert_eq!(result.to_string(), "hello");
    }

    #[test]
    fn test_default_val_undefined() {
        let result = default_val(Value::UNDEFINED, Value::from("fallback"));
        assert_eq!(result.to_string(), "fallback");
    }

    #[test]
    fn test_default_val_empty_string() {
        let result = default_val(Value::from(""), Value::from("fallback"));
        assert_eq!(result.to_string(), "fallback");
    }

    #[test]
    fn test_trim_whitespace() {
        assert_eq!(trim("  hello  "), "hello");
    }

    #[test]
    fn test_trim_newlines() {
        assert_eq!(trim("\n  hello  \n"), "hello");
    }

    #[test]
    fn test_trim_already_trimmed() {
        assert_eq!(trim("hello"), "hello");
    }

    #[test]
    fn test_lines_multiline() {
        let result = lines("line1\nline2\nline3");
        assert_eq!(result, vec!["line1", "line2", "line3"]);
    }

    #[test]
    fn test_lines_single() {
        let result = lines("single");
        assert_eq!(result, vec!["single"]);
    }

    #[test]
    fn test_lines_empty() {
        let result = lines("");
        assert_eq!(result, Vec::<String>::new());
    }
}
