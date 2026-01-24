//! Logic for discovering and resolving systemd services based on the filesystem.

use super::service::{get_bare_name, get_compose_dir};
use crate::constants::COMPOSE_FILES;
use crate::core::Context;
use anyhow::{bail, Result};
use std::env;

/// Attempts to detect a service name based on the current working directory.
///
/// Returns a service name if the CWD is within the `compose_base` and contains
/// a recognized Docker Compose file.
///
/// # Arguments
///
/// * `ctx` - The application context.
pub fn detect_service_from_cwd(ctx: &Context) -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let rel_path = cwd.strip_prefix(&ctx.compose_base).ok()?;

    let has_compose_file = COMPOSE_FILES.iter().any(|f| cwd.join(f).exists());
    if !has_compose_file {
        return None;
    }

    let service_name = rel_path
        .to_str()? // This is a valid escape sequence for a newline in Rust string literals.
        .replace([std::path::MAIN_SEPARATOR, '/'], "-");

    Some(service_name)
}

/// Resolves a list of service names, defaulting to CWD detection if the list is empty.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - An optional list of explicit service names.
///
/// # Errors
///
/// Returns an error if no services are specified and detection fails.
pub fn resolve_services(ctx: &Context, services: &[String]) -> Result<Vec<String>> {
    if !services.is_empty() {
        return Ok(services.to_vec());
    }

    if let Some(service) = detect_service_from_cwd(ctx) {
        println!("Auto-detected service: {}", service);
        return Ok(vec![service]);
    }

    bail!(
        "No service specified and current directory is not a compose project.\n\nEither specify a service name or run from a directory under {}",
        ctx.compose_base.display()
    );
}

/// Resolves a single service name, defaulting to CWD detection if the name is empty.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `service` - An explicit service name.
///
/// # Errors
///
/// Returns an error if no service is specified and detection fails.
pub fn resolve_service(ctx: &Context, service: &str) -> Result<String> {
    if !service.is_empty() {
        return Ok(service.to_string());
    }

    if let Some(detected) = detect_service_from_cwd(ctx) {
        println!("Auto-detected service: {}", detected);
        return Ok(detected);
    }

    bail!(
        "No service specified and current directory is not a compose project.\n\nEither specify a service name or run from a directory under {}",
        ctx.compose_base.display()
    );
}

/// Validates that all provided services have existing compose directories.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `services` - A list of service names to validate.
///
/// # Errors
///
/// Returns an error if any of the service directories do not exist.
pub fn validate_compose_dirs(ctx: &Context, services: &[String]) -> Result<()> {
    let mut missing = Vec::new();

    for service in services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);
        if !dir.exists() {
            missing.push((bare.to_string(), dir));
        }
    }

    if !missing.is_empty() {
        let msg = missing
            .iter()
            .map(|(name, path)| format!("  - '{}' (expected at {})", name, path.display()))
            .collect::<Vec<_>>()
            .join("\n");
        bail!(
            "Compose directory not found for the following services:\n{}\n\nEnsure the service name matches an existing directory under {}",
            msg,
            ctx.compose_base.display()
        );
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_context(compose_base: &Path) -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: compose_base.to_path_buf(),
            env_file: PathBuf::from("/tmp/test-compose.env"),
            docker_host: None,
        }
    }

    struct TestDir {
        base: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let base = PathBuf::from(format!("/tmp/compose-test-{}-{}", name, std::process::id()));

            fs::create_dir_all(&base).unwrap();

            Self { base }
        }

        fn create_dir(&self, path: &str) -> PathBuf {
            let dir = self.base.join(path);
            fs::create_dir_all(&dir).unwrap();
            dir
        }

        fn path(&self) -> &Path {
            &self.base
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    #[test]
    fn test_validate_compose_dirs_exists() {
        let test_dir = TestDir::new("validate-exists");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        let result = validate_compose_dirs(&ctx, &["myapp".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_compose_dirs_not_exists() {
        let test_dir = TestDir::new("validate-not-exists");
        let ctx = test_context(test_dir.path());
        let result = validate_compose_dirs(&ctx, &["nonexistent".to_string()]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("nonexistent"));
        assert!(err.contains("not found"));
    }

    #[test]
    fn test_validate_compose_dirs_multiple() {
        let test_dir = TestDir::new("validate-multiple");
        test_dir.create_dir("app1");
        test_dir.create_dir("app2");
        let ctx = test_context(test_dir.path());
        let result = validate_compose_dirs(&ctx, &["app1".to_string(), "app2".to_string()]);
        assert!(result.is_ok());
    }

    #[test]
    fn test_validate_compose_dirs_partial_missing() {
        let test_dir = TestDir::new("validate-partial");
        test_dir.create_dir("app1");
        let ctx = test_context(test_dir.path());
        let result = validate_compose_dirs(&ctx, &["app1".to_string(), "app2".to_string()]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("app2"));
        assert!(!err.contains("app1"));
    }

    #[test]
    fn test_resolve_services_with_explicit_services() {
        let test_dir = TestDir::new("resolve-explicit");
        let ctx = test_context(test_dir.path());
        let result = resolve_services(&ctx, &["app1".to_string(), "app2".to_string()]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["app1", "app2"]);
    }

    #[test]
    fn test_resolve_services_empty_not_in_project() {
        let test_dir = TestDir::new("resolve-empty-no-project");
        let ctx = test_context(test_dir.path());
        let result = resolve_services(&ctx, &[]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("No service specified"));
    }

    #[test]
    fn test_detect_service_not_under_compose_base() {
        let test_dir = TestDir::new("detect-not-under");
        let ctx = test_context(test_dir.path());
        let result = detect_service_from_cwd(&ctx);
        assert!(result.is_none());
    }

    #[test]
    fn test_resolve_service_with_explicit() {
        let test_dir = TestDir::new("resolve-single-explicit");
        let ctx = test_context(test_dir.path());
        let result = resolve_service(&ctx, "myapp");
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), "myapp");
    }

    #[test]
    fn test_resolve_service_empty_not_in_project() {
        let test_dir = TestDir::new("resolve-single-empty");
        let ctx = test_context(test_dir.path());
        let result = resolve_service(&ctx, "");
        assert!(result.is_err());
    }

    #[test]
    fn test_roundtrip_simple_name() {
        use super::super::service::normalize_unit_name;
        let test_dir = TestDir::new("roundtrip-simple");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());

        let initial_bare = "myapp";
        let service_name = normalize_unit_name(&ctx, initial_bare);
        assert_eq!(service_name, "compose@myapp.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "myapp");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("myapp"));
    }

    #[test]
    fn test_roundtrip_nested_name() {
        use super::super::service::normalize_unit_name;
        let test_dir = TestDir::new("roundtrip-nested");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());

        let service_name = normalize_unit_name(&ctx, "genai/ollama");
        assert_eq!(service_name, "compose@genai-ollama.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "genai-ollama");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("genai/ollama"));
    }

    #[test]
    fn test_roundtrip_dash_input_to_nested() {
        use super::super::service::normalize_unit_name;
        let test_dir = TestDir::new("roundtrip-dash");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());

        let service_name = normalize_unit_name(&ctx, "genai-ollama");
        assert_eq!(service_name, "compose@genai-ollama.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "genai-ollama");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("genai/ollama"));
    }

    #[test]
    fn test_roundtrip_flat_with_dash() {
        use super::super::service::normalize_unit_name;
        let test_dir = TestDir::new("roundtrip-flat-dash");
        test_dir.create_dir("my-project");
        let ctx = test_context(test_dir.path());

        let service_name = normalize_unit_name(&ctx, "my-project");
        assert_eq!(service_name, "compose@my-project.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "my-project");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("my-project"));
    }
}
