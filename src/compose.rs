//! Logic for parsing dependency configuration files.

use anyhow::{Context, Result};
use serde::Deserialize;
use std::collections::HashMap;
use std::fs;
use std::path::Path;

/// Represents a single service's dependency configuration.
#[derive(Debug, Deserialize)]
pub struct ServiceConfig {
    /// List of services that must be started before this service.
    /// Maps to `Requires=` and `After=` in systemd.
    pub requires: Option<Vec<String>>,

    /// List of services that this service wants to start with.
    /// Maps to `Wants=` and `After=` in systemd.
    pub wants: Option<Vec<String>>,

    /// List of services that this service is bound to.
    /// Maps to `BindsTo=` and `After=` in systemd.
    pub binds_to: Option<Vec<String>>,

    /// List of services that this service should start after.
    /// Maps explicitly to `After=` in systemd.
    pub after: Option<Vec<String>>,
}

/// Represents the top-level structure of the dependencies TOML file.
#[derive(Debug, Deserialize)]
pub struct DependenciesConfig {
    /// Map of service names to their dependency configuration.
    /// The TOML key should be `[dependencies.<service_name>]`.
    #[serde(default, rename = "dependencies")]
    pub services: HashMap<String, ServiceConfig>,
}

/// Loads and parses a dependency configuration file.
///
/// # Arguments
///
/// * `path` - Path to the TOML configuration file.
pub fn load_dependencies(path: &Path) -> Result<DependenciesConfig> {
    let content = fs::read_to_string(path)
        .with_context(|| format!("Failed to read dependency file: {}", path.display()))?;

    let config: DependenciesConfig = toml::from_str(&content)
        .with_context(|| format!("Failed to parse dependency file: {}", path.display()))?;

    Ok(config)
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::io::Write;
    use tempfile::NamedTempFile;

    #[test]
    fn test_parse_simple_dependencies() {
        let toml_content = r#"
            [dependencies.bifrost]
            requires = ["pgvector"]

            [dependencies.open-webui]
            requires = ["pgvector", "bifrost"]
            wants = ["ollama", "qdrant", "classifier"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();

        assert_eq!(config.services.len(), 2);

        let bifrost = config.services.get("bifrost").unwrap();
        assert_eq!(bifrost.requires.as_ref().unwrap(), &vec!["pgvector"]);
        assert!(bifrost.wants.is_none());

        let webui = config.services.get("open-webui").unwrap();
        assert_eq!(
            webui.requires.as_ref().unwrap(),
            &vec!["pgvector", "bifrost"]
        );
        assert_eq!(
            webui.wants.as_ref().unwrap(),
            &vec!["ollama", "qdrant", "classifier"]
        );
    }

    #[test]
    fn test_parse_expanded_dependencies() {
        let toml_content = r#"
            [dependencies.db]
            binds_to = ["docker"]
            after = ["docker", "network"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        let db = config.services.get("db").unwrap();

        assert_eq!(db.binds_to.as_ref().unwrap(), &vec!["docker"]);
        assert_eq!(db.after.as_ref().unwrap(), &vec!["docker", "network"]);
    }

    #[test]
    fn test_parse_empty() {
        let toml_content = "";
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        assert!(config.services.is_empty());
    }

    #[test]
    fn test_parse_all_fields() {
        let toml_content = r#"
            [dependencies.myapp]
            requires = ["db"]
            wants = ["cache"]
            binds_to = ["docker.service"]
            after = ["network.target"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        let myapp = config.services.get("myapp").unwrap();
        assert_eq!(myapp.requires.as_ref().unwrap(), &vec!["db"]);
        assert_eq!(myapp.wants.as_ref().unwrap(), &vec!["cache"]);
        assert_eq!(myapp.binds_to.as_ref().unwrap(), &vec!["docker.service"]);
        assert_eq!(myapp.after.as_ref().unwrap(), &vec!["network.target"]);
    }

    #[test]
    fn test_parse_only_optional_fields() {
        let toml_content = r#"
            [dependencies.myapp]
            wants = ["cache"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        let myapp = config.services.get("myapp").unwrap();
        assert!(myapp.requires.is_none());
        assert_eq!(myapp.wants.as_ref().unwrap(), &vec!["cache"]);
        assert!(myapp.binds_to.is_none());
        assert!(myapp.after.is_none());
    }

    #[test]
    fn test_parse_nonexistent_file() {
        let result = load_dependencies(Path::new("/tmp/nonexistent-deps-file-12345.toml"));
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to read dependency file"));
    }

    #[test]
    fn test_parse_invalid_toml() {
        let mut file = NamedTempFile::new().unwrap();
        write!(file, "[invalid toml {{{{").unwrap();

        let result = load_dependencies(file.path());
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("Failed to parse dependency file"));
    }

    #[test]
    fn test_parse_wrong_structure() {
        // Valid TOML but wrong structure (not under [dependencies])
        let toml_content = r#"
            [services.myapp]
            requires = ["db"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        // Should parse but have no services (wrong section name)
        assert!(config.services.is_empty());
    }

    #[test]
    fn test_parse_empty_dependency_lists() {
        let toml_content = r#"
            [dependencies.myapp]
            requires = []
            wants = []
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        let myapp = config.services.get("myapp").unwrap();
        assert!(myapp.requires.as_ref().unwrap().is_empty());
        assert!(myapp.wants.as_ref().unwrap().is_empty());
    }

    #[test]
    fn test_parse_many_services() {
        let mut toml_content = String::new();
        for i in 0..20 {
            toml_content.push_str(&format!(
                "[dependencies.service-{}]\nrequires = [\"dep-{}\"]\n\n",
                i, i
            ));
        }

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        assert_eq!(config.services.len(), 20);
    }

    #[test]
    fn test_parse_service_name_with_special_chars() {
        let toml_content = r#"
            [dependencies."my-app_v2.0"]
            requires = ["db"]
        "#;

        let mut file = NamedTempFile::new().unwrap();
        write!(file, "{}", toml_content).unwrap();

        let config = load_dependencies(file.path()).unwrap();
        assert!(config.services.contains_key("my-app_v2.0"));
    }
}
