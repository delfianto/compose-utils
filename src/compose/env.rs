//! Utilities for handling environment variables and `.env` files.

use anyhow::Result;
use once_cell::sync::Lazy;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Regex for matching environment variable placeholders ($VAR or ${VAR}).
static ENV_VAR_RE: Lazy<Regex> =
    Lazy::new(|| Regex::new(r"\$\{?([a-zA-Z_][a-zA-Z0-9_]*)\}?").unwrap());

/// Loads environment variables from a specified `.env` file.
///
/// - Ignores empty lines and comments (starting with `#`).
/// - Trims whitespace from keys and values.
/// - Strips surrounding quotes (double or single) from values.
///
/// # Arguments
///
/// * `path` - The path to the `.env` file to read.
///
/// # Errors
///
/// Returns an error if the file cannot be read.
pub fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
    let content = fs::read_to_string(path)?;
    let mut vars = HashMap::new();

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            let clean_value = value.trim_matches('"').trim_matches('\'');
            vars.insert(key.trim().to_string(), clean_value.to_string());
        }
    }
    Ok(vars)
}

/// Resolves environment variable placeholders in a string.
///
/// Placeholders can be in the form `${VAR}` or `$VAR`. If a variable is not
/// found in the provided map, the placeholder is left as-is.
///
/// # Arguments
///
/// * `text` - The string containing potential placeholders.
/// * `vars` - A map containing variable keys and their replacement values.
pub fn resolve_env_vars(text: &str, vars: &HashMap<String, String>) -> String {
    ENV_VAR_RE
        .replace_all(text, |caps: &regex::Captures| {
            let key = &caps[1];
            vars.get(key)
                .cloned()
                .unwrap_or_else(|| caps[0].to_string())
        })
        .to_string()
}

/// Finds environment variable placeholders in a string that were not resolved.
///
/// Returns a list of variable names (without the `$` or `${}` syntax) that
/// still appear as placeholders in the text.
///
/// # Arguments
///
/// * `text` - The string to check for unresolved placeholders.
pub fn find_unresolved_vars(text: &str) -> Vec<String> {
    ENV_VAR_RE
        .captures_iter(text)
        .map(|caps| caps[1].to_string())
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_resolve_env_vars_with_braces() {
        let mut vars = HashMap::new();
        vars.insert("TAG".to_string(), "v1.0".to_string());
        vars.insert("REGISTRY".to_string(), "docker.io".to_string());

        assert_eq!(resolve_env_vars("nginx:${TAG}", &vars), "nginx:v1.0");
        assert_eq!(
            resolve_env_vars("${REGISTRY}/app:latest", &vars),
            "docker.io/app:latest"
        );
    }

    #[test]
    fn test_resolve_env_vars_without_braces() {
        let mut vars = HashMap::new();
        vars.insert("TAG".to_string(), "v1.0".to_string());

        assert_eq!(resolve_env_vars("nginx:$TAG", &vars), "nginx:v1.0");
    }

    #[test]
    fn test_resolve_env_vars_no_substitution() {
        let vars = HashMap::new();
        assert_eq!(resolve_env_vars("postgres:13", &vars), "postgres:13");
    }

    #[test]
    fn test_resolve_env_vars_missing_var() {
        let vars = HashMap::new();
        assert_eq!(resolve_env_vars("app:${MISSING}", &vars), "app:${MISSING}");
    }

    #[test]
    fn test_resolve_env_vars_multiple() {
        let mut vars = HashMap::new();
        vars.insert("REGISTRY".to_string(), "gcr.io".to_string());
        vars.insert("PROJECT".to_string(), "myproject".to_string());
        vars.insert("TAG".to_string(), "latest".to_string());

        assert_eq!(
            resolve_env_vars("${REGISTRY}/${PROJECT}/app:${TAG}", &vars),
            "gcr.io/myproject/app:latest"
        );
    }

    #[test]
    fn test_load_env_file_basic() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "TAG=v1.0").unwrap();
        writeln!(file, "USER=admin").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.get("TAG"), Some(&"v1.0".to_string()));
        assert_eq!(vars.get("USER"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_load_env_file_with_comments() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "# This is a comment").unwrap();
        writeln!(file, "TAG=v1.0").unwrap();
        writeln!(file).unwrap();
        writeln!(file, "# Another comment").unwrap();
        writeln!(file, "USER=admin").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("TAG"), Some(&"v1.0".to_string()));
        assert_eq!(vars.get("USER"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_load_env_file_with_quotes() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "DOUBLE=\"quoted value\"").unwrap();
        writeln!(file, "SINGLE='single quoted'").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.get("DOUBLE"), Some(&"quoted value".to_string()));
        assert_eq!(vars.get("SINGLE"), Some(&"single quoted".to_string()));
    }

    #[test]
    fn test_load_env_file_malformed() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "VALID_KEY=value").unwrap();
        writeln!(file, "INVALID_LINE_NO_EQUALS").unwrap();
        writeln!(file, "=MISSING_KEY").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.len(), 2);
        assert_eq!(vars.get("VALID_KEY"), Some(&"value".to_string()));
        assert_eq!(vars.get(""), Some(&"MISSING_KEY".to_string()));
    }

    #[test]
    fn test_load_env_file_multiple_equals() {
        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "COMPLEX_VAL=key1=val1;key2=val2").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(
            vars.get("COMPLEX_VAL"),
            Some(&"key1=val1;key2=val2".to_string())
        );
    }
}
