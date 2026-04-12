//! Logic for interacting with systemd services and resolving service names.

use crate::core::Context;
use crate::verbose;
use std::path::PathBuf;

/// Converts a project name to its corresponding directory path.
///
/// Handles both literal paths (e.g., `genai/ollama`) and flat names that
/// map to nested structures (e.g., `genai-ollama` -> `genai/ollama`).
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `name` - The project name or path segment.
pub fn name_to_dir_path(ctx: &Context, name: &str) -> String {
    if name.contains('/') {
        return name.to_string();
    }

    let literal_path = ctx.compose_base.join(name);
    if literal_path.exists() {
        return name.to_string();
    }

    let converted = name.replace('-', "/");
    let converted_path = ctx.compose_base.join(&converted);
    if converted_path.exists() {
        verbose!(
            "Resolved flat name '{}' to nested path '{}'",
            name,
            converted
        );
        return converted;
    }

    name.to_string()
}

/// Normalizes a name into a full systemd unit name.
///
/// The normalization strategy is:
/// 1. If it's already a full `compose@` unit name, return it.
/// 2. If it looks like a standard unit (no slash, has extension or @), return it.
/// 3. Otherwise, try to resolve it as a compose project.
///    - If it matches a directory (nested or flat), format as `compose@<flat-name>.service`.
///    - If not, fallback to a standard `.service` suffix.
pub fn normalize_unit_name(ctx: &Context, name: &str) -> String {
    // Already fully qualified correctly.
    if name.starts_with("compose@") && name.ends_with(".service") && !name.contains('/') {
        return name.to_string();
    }

    // Likely standard systemd unit (must NOT have slash, as our projects do).
    if !name.starts_with("compose@")
        && !name.contains('/')
        && (name.contains('@')
            || name.ends_with(".target")
            || name.ends_with(".socket")
            || name.ends_with(".timer")
            || name.ends_with(".service"))
    {
        return name.to_string();
    }

    // Extract the inner part if it has prefixes/suffixes.
    let mut inner = name;
    if let Some(stripped) = name.strip_prefix("compose@") {
        inner = stripped;
    }

    // Determine if it's a compose project.
    let bare = inner.strip_suffix(".service").unwrap_or(inner);
    let potential_dir = name_to_dir_path(ctx, bare);
    let project_exists = ctx.compose_base.join(&potential_dir).exists();

    let normalized = if project_exists || bare.contains('/') || name.starts_with("compose@") {
        let normalized = bare.replace('/', "-");
        format!("compose@{}.service", normalized)
    } else {
        // Fallback for names like "docker" or "database" that are not projects.
        if inner.ends_with(".service") {
            inner.to_string()
        } else {
            format!("{}.service", inner)
        }
    };

    if normalized != name {
        verbose!("Normalized '{}' -> '{}'", name, normalized);
    }
    normalized
}

/// Extracts the bare project name from a service unit string.
///
/// Strips the `compose@` prefix and `.service` suffix if they exist.
///
/// # Arguments
///
/// * `service` - The service unit name (e.g., `compose@myapp.service`).
pub fn get_bare_name(service: &str) -> &str {
    let s = service.strip_suffix(".service").unwrap_or(service);
    s.strip_prefix("compose@").unwrap_or(s)
}

/// Resolves the absolute filesystem path for a compose project's directory.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `name` - The project name.
pub fn get_compose_dir(ctx: &Context, name: &str) -> PathBuf {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);
    ctx.compose_base.join(dir_path)
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Context;
    use std::fs;
    use std::path::{Path, PathBuf};

    /// Creates a mock Context for testing.
    fn test_context(compose_base: &Path) -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            compose_base: compose_base.to_path_buf(),
            env_file: PathBuf::from("/tmp/test-compose.env"),
            docker_host: None,
            infisical_project_id: None,
            infisical_env: None,
            infisical_address: None,
            infisical_bootstrap: vec![],
        }
    }

    /// Helper struct for managing a temporary directory in tests.
    struct TestDir {
        base: PathBuf,
    }

    impl TestDir {
        /// Creates a new TestDir with a unique name.
        fn new(name: &str) -> Self {
            let base = PathBuf::from(format!("/tmp/compose-test-{}-{}", name, std::process::id()));
            fs::create_dir_all(&base).unwrap();
            Self { base }
        }

        /// Creates a subdirectory within the TestDir.
        fn create_dir(&self, path: &str) -> PathBuf {
            let dir = self.base.join(path);
            fs::create_dir_all(&dir).unwrap();
            dir
        }

        /// Returns the path to the TestDir.
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
    fn test_normalize_unit_name_simple() {
        let test_dir = TestDir::new("norm-simple");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        assert_eq!(normalize_unit_name(&ctx, "myapp"), "compose@myapp.service");
    }

    #[test]
    fn test_normalize_unit_name_with_dash() {
        let test_dir = TestDir::new("norm-dash");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());
        assert_eq!(
            normalize_unit_name(&ctx, "genai-ollama"),
            "compose@genai-ollama.service"
        );
    }

    #[test]
    fn test_normalize_unit_name_with_slash() {
        let test_dir = TestDir::new("norm-slash");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());
        assert_eq!(
            normalize_unit_name(&ctx, "genai/ollama"),
            "compose@genai-ollama.service"
        );
    }

    #[test]
    fn test_normalize_unit_name_standard() {
        let test_dir = TestDir::new("norm-std");
        let ctx = test_context(test_dir.path());
        assert_eq!(
            normalize_unit_name(&ctx, "docker.service"),
            "docker.service"
        );
        assert_eq!(
            normalize_unit_name(&ctx, "network.target"),
            "network.target"
        );
        assert_eq!(
            normalize_unit_name(&ctx, "user@1000.service"),
            "user@1000.service"
        );
    }

    #[test]
    fn test_normalize_unit_name_fallback() {
        let test_dir = TestDir::new("norm-fallback");
        let ctx = test_context(test_dir.path());
        // "docker" is NOT a project, so it should be "docker.service"
        assert_eq!(normalize_unit_name(&ctx, "docker"), "docker.service");
        assert_eq!(normalize_unit_name(&ctx, "database"), "database.service");
    }

    #[test]
    fn test_normalize_unit_name_redundant() {
        let test_dir = TestDir::new("norm-red");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        assert_eq!(
            normalize_unit_name(&ctx, "compose@myapp.service"),
            "compose@myapp.service"
        );
    }

    #[test]
    fn test_normalize_unit_name_nested_deep() {
        let test_dir = TestDir::new("norm-deep");
        test_dir.create_dir("a/b/c/d");
        let ctx = test_context(test_dir.path());
        assert_eq!(
            normalize_unit_name(&ctx, "a/b/c/d"),
            "compose@a-b-c-d.service"
        );
    }

    #[test]
    fn test_get_bare_name_simple() {
        assert_eq!(get_bare_name("myapp"), "myapp");
    }

    #[test]
    fn test_get_bare_name_with_service_suffix() {
        assert_eq!(get_bare_name("myapp.service"), "myapp");
    }

    #[test]
    fn test_get_bare_name_with_prefix() {
        assert_eq!(get_bare_name("compose@myapp"), "myapp");
    }

    #[test]
    fn test_get_bare_name_full_service_name() {
        assert_eq!(get_bare_name("compose@myapp.service"), "myapp");
    }

    #[test]
    fn test_get_bare_name_with_dash() {
        assert_eq!(get_bare_name("compose@my-app.service"), "my-app");
    }

    #[test]
    fn test_name_to_dir_path_with_slash() {
        let test_dir = TestDir::new("dir-path-slash");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "genai/ollama"), "genai/ollama");
    }

    #[test]
    fn test_name_to_dir_path_flat_exists() {
        let test_dir = TestDir::new("dir-path-flat");
        test_dir.create_dir("my-project");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "my-project"), "my-project");
    }

    #[test]
    fn test_name_to_dir_path_nested_exists() {
        let test_dir = TestDir::new("dir-path-nested");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "genai-ollama"), "genai/ollama");
    }

    #[test]
    fn test_name_to_dir_path_converted() {
        let test_dir = TestDir::new("dir-path-conv");
        test_dir.create_dir("a/b/c");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "a-b-c"), "a/b/c");
    }

    #[test]
    fn test_name_to_dir_path_neither_exists() {
        let test_dir = TestDir::new("dir-path-none");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "nonexistent"), "nonexistent");
    }

    #[test]
    fn test_name_to_dir_path_prefers_flat_over_nested() {
        let test_dir = TestDir::new("dir-path-prefer");
        test_dir.create_dir("my-project");
        test_dir.create_dir("my/project");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "my-project"), "my-project");
    }

    #[test]
    fn test_get_compose_dir_simple() {
        let test_dir = TestDir::new("compose-dir-simple");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        let dir = get_compose_dir(&ctx, "myapp");
        assert_eq!(dir, test_dir.path().join("myapp"));
    }

    #[test]
    fn test_get_compose_dir_nested() {
        let test_dir = TestDir::new("compose-dir-nested");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());
        let dir = get_compose_dir(&ctx, "genai-ollama");
        assert_eq!(dir, test_dir.path().join("genai/ollama"));
    }

    #[test]
    fn test_get_compose_dir_strips_service_name() {
        let test_dir = TestDir::new("compose-dir-strips");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        let dir = get_compose_dir(&ctx, "compose@myapp.service");
        assert_eq!(dir, test_dir.path().join("myapp"));
    }

    // --- Idempotency tests ---

    #[test]
    fn test_normalize_idempotent() {
        let test_dir = TestDir::new("norm-idempotent");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());
        let once = normalize_unit_name(&ctx, "myapp");
        let twice = normalize_unit_name(&ctx, &once);
        assert_eq!(once, twice);
    }

    #[test]
    fn test_normalize_idempotent_nested() {
        let test_dir = TestDir::new("norm-idempotent-nested");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());
        let once = normalize_unit_name(&ctx, "genai/ollama");
        let twice = normalize_unit_name(&ctx, &once);
        assert_eq!(once, twice);
    }

    // --- get_bare_name edge cases ---

    #[test]
    fn test_get_bare_name_empty_string() {
        assert_eq!(get_bare_name(""), "");
    }

    #[test]
    fn test_get_bare_name_only_service_suffix() {
        assert_eq!(get_bare_name(".service"), "");
    }

    #[test]
    fn test_get_bare_name_only_prefix() {
        assert_eq!(get_bare_name("compose@"), "");
    }

    #[test]
    fn test_get_bare_name_standard_service() {
        // Non-compose service names pass through stripped of .service
        assert_eq!(get_bare_name("docker.service"), "docker");
    }

    // --- normalize_unit_name with timer/socket/target ---

    #[test]
    fn test_normalize_preserves_timer() {
        let test_dir = TestDir::new("norm-timer");
        let ctx = test_context(test_dir.path());
        assert_eq!(normalize_unit_name(&ctx, "cleanup.timer"), "cleanup.timer");
    }

    #[test]
    fn test_normalize_preserves_socket() {
        let test_dir = TestDir::new("norm-socket");
        let ctx = test_context(test_dir.path());
        assert_eq!(normalize_unit_name(&ctx, "dbus.socket"), "dbus.socket");
    }

    // --- name_to_dir_path edge cases ---

    #[test]
    fn test_name_to_dir_path_empty_string() {
        let test_dir = TestDir::new("dir-path-empty");
        let ctx = test_context(test_dir.path());
        // Empty string - no directory exists, returns as-is
        assert_eq!(name_to_dir_path(&ctx, ""), "");
    }

    #[test]
    fn test_name_to_dir_path_multiple_dashes() {
        let test_dir = TestDir::new("dir-path-multi-dash");
        test_dir.create_dir("a/b/c/d");
        let ctx = test_context(test_dir.path());
        assert_eq!(name_to_dir_path(&ctx, "a-b-c-d"), "a/b/c/d");
    }

    // --- get_compose_dir with various inputs ---

    #[test]
    fn test_get_compose_dir_nonexistent() {
        let test_dir = TestDir::new("compose-dir-noexist");
        let ctx = test_context(test_dir.path());
        // Should still return a path, even if it doesn't exist
        let dir = get_compose_dir(&ctx, "nonexistent");
        assert_eq!(dir, test_dir.path().join("nonexistent"));
    }
}
