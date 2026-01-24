//! Utilities for handling environment variables and `.env` files.

use anyhow::Result;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

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
            // Remove potential quotes
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
    let re = Regex::new(r"\$\{?([a-zA-Z_][a-zA-Z0-9_]*)\}?").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let key = &caps[1];
        vars.get(key)
            .cloned()
            .unwrap_or_else(|| caps[0].to_string())
    })
    .to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

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
        // Missing variables should be kept as-is
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
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "TAG=v1.0").unwrap();
        writeln!(file, "USER=admin").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.get("TAG"), Some(&"v1.0".to_string()));
        assert_eq!(vars.get("USER"), Some(&"admin".to_string()));
    }

    #[test]
    fn test_load_env_file_with_comments() {
        use std::io::Write;
        use tempfile::NamedTempFile;

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
        use std::io::Write;
        use tempfile::NamedTempFile;

        let mut file = NamedTempFile::new().unwrap();
        writeln!(file, "DOUBLE=\"quoted value\"").unwrap();
        writeln!(file, "SINGLE='single quoted'").unwrap();

        let vars = load_env_file(file.path()).unwrap();
        assert_eq!(vars.get("DOUBLE"), Some(&"quoted value".to_string()));
        assert_eq!(vars.get("SINGLE"), Some(&"single quoted".to_string()));
    }
}
