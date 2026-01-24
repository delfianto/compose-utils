use super::env::{load_env_file, resolve_env_vars};
use super::types::DockerCompose;
use crate::constants::COMPOSE_FILES;
use anyhow::{Context, Result};
use std::collections::HashMap;
use std::fs;
use std::path::{Path, PathBuf};

/// Find compose file in a directory
pub fn find_compose_file(dir: &Path) -> Option<PathBuf> {
    for name in COMPOSE_FILES {
        let path = dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Get images from a compose project directory
pub fn get_images_for_project(project_dir: &Path) -> Result<Vec<String>> {
    let compose_file = find_compose_file(project_dir)
        .ok_or_else(|| anyhow::anyhow!("No compose file found in {:?}", project_dir))?;
    let env_file = project_dir.join(".env");

    // Load .env if present
    let env_vars = if env_file.exists() {
        load_env_file(&env_file)?
    } else {
        HashMap::new()
    };

    // Parse compose file
    let content = fs::read_to_string(&compose_file)
        .with_context(|| format!("Failed to read {:?}", compose_file))?;

    let compose: DockerCompose = serde_yaml_ng::from_str(&content)
        .with_context(|| format!("Failed to parse {:?}", compose_file))?;

    let mut images = Vec::new();
    if let Some(services) = compose.services {
        for (_, service) in services {
            if let Some(raw_image) = service.image {
                let resolved = resolve_env_vars(&raw_image, &env_vars);
                images.push(resolved);
            }
        }
    }

    Ok(images)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_find_compose_file_docker_compose_yml() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("docker-compose.yml"), "").unwrap();

        let result = find_compose_file(temp_dir.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("docker-compose.yml"));
    }

    #[test]
    fn test_find_compose_file_compose_yaml() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        fs::write(temp_dir.path().join("compose.yaml"), "").unwrap();

        let result = find_compose_file(temp_dir.path());
        assert!(result.is_some());
        assert!(result.unwrap().ends_with("compose.yaml"));
    }

    #[test]
    fn test_find_compose_file_none() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let result = find_compose_file(temp_dir.path());
        assert!(result.is_none());
    }

    #[test]
    fn test_get_images_for_project() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create compose file
        let compose_content = r#"
services:
  web:
    image: nginx:${TAG}
  db:
    image: postgres:14
"#;
        fs::write(project_dir.join("docker-compose.yml"), compose_content).unwrap();

        // Create .env file
        fs::write(project_dir.join(".env"), "TAG=1.25").unwrap();

        let images = get_images_for_project(project_dir).unwrap();
        assert!(images.contains(&"nginx:1.25".to_string()));
        assert!(images.contains(&"postgres:14".to_string()));
    }

    #[test]
    fn test_get_images_for_project_no_env() {
        use tempfile::TempDir;

        let temp_dir = TempDir::new().unwrap();
        let project_dir = temp_dir.path();

        // Create compose file without .env
        let compose_content = r#"
services:
  web:
    image: nginx:latest
"#;
        fs::write(project_dir.join("compose.yaml"), compose_content).unwrap();

        let images = get_images_for_project(project_dir).unwrap();
        assert_eq!(images, vec!["nginx:latest".to_string()]);
    }
}
