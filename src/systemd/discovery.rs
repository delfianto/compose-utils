//! Logic for discovering and resolving systemd services based on the filesystem.

use crate::core::COMPOSE_FILES;
use crate::core::Context;
use crate::verbose;
use anyhow::{Result, bail};
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
    verbose!("Attempting to detect service from CWD: {}", cwd.display());
    let rel_path = cwd.strip_prefix(&ctx.compose_base).ok()?;

    let has_compose_file = COMPOSE_FILES.iter().any(|f| cwd.join(f).exists());
    if !has_compose_file {
        verbose!("No compose file found in CWD");
        return None;
    }

    let service_name = rel_path
        .to_str()? // This is a valid escape sequence for a newline in Rust string literals.
        .replace([std::path::MAIN_SEPARATOR, '/'], "-");

    verbose!("Detected service name: {}", service_name);
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
        verbose!("Using explicit service list: {:?}", services);
        return Ok(services.to_vec());
    }

    if let Some(service) = detect_service_from_cwd(ctx) {
        if !crate::core::is_json() {
            println!("Auto-detected service: {}", service);
        }
        return Ok(vec![service]);
    }

    bail!(
        "No service specified and current directory is not a compose project.\n\nEither specify a service name or run from a directory under {}",
        ctx.compose_base.display()
    );
}

#[cfg(test)]
mod tests {
    use super::super::service::{get_bare_name, get_compose_dir};
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

    #[test]
    fn test_resolve_services_single_service() {
        let test_dir = TestDir::new("resolve-single");
        let ctx = test_context(test_dir.path());
        let result = resolve_services(&ctx, &["myapp".to_string()]);
        assert!(result.is_ok());
        assert_eq!(result.unwrap(), vec!["myapp"]);
    }

    #[test]
    fn test_resolve_services_preserves_order() {
        let test_dir = TestDir::new("resolve-order");
        let ctx = test_context(test_dir.path());
        let input = vec!["zz".to_string(), "aa".to_string(), "mm".to_string()];
        let result = resolve_services(&ctx, &input).unwrap();
        assert_eq!(result, vec!["zz", "aa", "mm"]);
    }

    #[test]
    fn test_resolve_services_error_message_includes_path() {
        let test_dir = TestDir::new("resolve-err-path");
        let ctx = test_context(test_dir.path());
        let result = resolve_services(&ctx, &[]);
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains(&test_dir.path().to_string_lossy().to_string()));
    }
}
