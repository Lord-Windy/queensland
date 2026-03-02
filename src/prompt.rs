//! Prompt template loading and interpolation.

use serde_json::Value;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::path::PathBuf;

#[derive(Debug)]
pub enum PromptError {
    NotFound(String),
    IoError(std::io::Error),
    ParseError(String),
}

impl std::fmt::Display for PromptError {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        match self {
            PromptError::NotFound(name) => write!(f, "Prompt not found: {}", name),
            PromptError::IoError(e) => write!(f, "IO error: {}", e),
            PromptError::ParseError(msg) => write!(f, "Parse error: {}", msg),
        }
    }
}

impl std::error::Error for PromptError {}

fn get_search_paths() -> Vec<PathBuf> {
    let mut paths = Vec::new();

    paths.push(env::current_dir().unwrap_or_default().join("prompts"));

    if let Some(home) = dirs::config_dir() {
        paths.push(home.join("queensland").join("prompts"));
    }

    paths
}

fn resolve_path(name: &str) -> Option<PathBuf> {
    for base in get_search_paths() {
        let path = base.join(name).with_extension("txt");
        if path.exists() {
            return Some(path);
        }
        let path = base.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

fn get_nested_value<'a>(value: &'a Value, path: &str) -> Option<&'a Value> {
    let parts: Vec<&str> = path.split('.').collect();
    let mut current = value;

    for part in parts {
        current = match current {
            Value::Object(map) => map.get(part)?,
            Value::Array(arr) => {
                let idx: usize = part.parse().ok()?;
                arr.get(idx)?
            }
            _ => return None,
        };
    }

    Some(current)
}

fn interpolate(template: &str, vars: &Value) -> Result<String, PromptError> {
    let mut result = String::new();
    let mut chars = template.chars().peekable();

    while let Some(ch) = chars.next() {
        if ch == '{' && chars.peek() == Some(&'{') {
            chars.next();

            let mut var_path = String::new();
            while let Some(&next) = chars.peek() {
                if next == '}' {
                    chars.next();
                    if chars.peek() == Some(&'}') {
                        chars.next();
                        break;
                    } else {
                        var_path.push('}');
                        continue;
                    }
                }
                var_path.push(chars.next().unwrap());
            }

            let trimmed = var_path.trim();
            if trimmed.is_empty() {
                result.push_str("{{}}");
            } else if let Some(value) = get_nested_value(vars, trimmed) {
                match value {
                    Value::String(s) => result.push_str(s),
                    Value::Number(n) => result.push_str(&n.to_string()),
                    Value::Bool(b) => result.push_str(&b.to_string()),
                    Value::Null => {}
                    _ => result.push_str(&value.to_string()),
                }
            } else {
                result.push_str(&format!("{{{{{}}}}}", trimmed));
            }
        } else {
            result.push(ch);
        }
    }

    Ok(result)
}

pub fn load_prompt(name: &str, vars: &Value) -> Result<String, PromptError> {
    let path = resolve_path(name).ok_or_else(|| PromptError::NotFound(name.to_string()))?;

    let template = fs::read_to_string(&path).map_err(PromptError::IoError)?;

    interpolate(&template, vars)
}

pub fn load_prompt_from_map(
    name: &str,
    vars: HashMap<String, Value>,
) -> Result<String, PromptError> {
    load_prompt(
        name,
        &Value::Object(vars.into_iter().map(|(k, v)| (k, v)).collect()),
    )
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn test_interpolate_simple() {
        let template = "Hello, {{name}}!";
        let vars = json!({"name": "World"});
        assert_eq!(interpolate(template, &vars).unwrap(), "Hello, World!");
    }

    #[test]
    fn test_interpolate_dot_notation() {
        let template = "User: {{user.name}}, Age: {{user.age}}";
        let vars = json!({
            "user": {
                "name": "Alice",
                "age": 30
            }
        });
        assert_eq!(
            interpolate(template, &vars).unwrap(),
            "User: Alice, Age: 30"
        );
    }

    #[test]
    fn test_interpolate_missing_var() {
        let template = "Hello, {{name}}!";
        let vars = json!({});
        assert_eq!(interpolate(template, &vars).unwrap(), "Hello, {{name}}!");
    }

    #[test]
    fn test_get_nested_value() {
        let vars = json!({
            "user": {
                "profile": {
                    "name": "Bob"
                }
            }
        });
        assert_eq!(
            get_nested_value(&vars, "user.profile.name"),
            Some(&json!("Bob"))
        );
    }

    #[test]
    fn test_get_nested_value_array() {
        let vars = json!({
            "items": ["a", "b", "c"]
        });
        assert_eq!(get_nested_value(&vars, "items.1"), Some(&json!("b")));
    }
}
