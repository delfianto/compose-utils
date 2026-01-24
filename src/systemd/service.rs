use crate::core::Context;
use anyhow::{Context as _, Result};
use std::path::PathBuf;
use std::process::Command;

/// Convert project name to directory path.
/// Handles both flat names (test-project) and nested names (genai-ollama -> genai/ollama).
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
        return converted;
    }

    name.to_string()
}

/// Convert project name to systemd service name.
/// Both `genai-ollama` and `genai/ollama` become `compose@genai-ollama.service`.
pub fn name_to_service(name: &str) -> String {
    let normalized = name.replace('/', "-");
    format!("compose@{}.service", normalized)
}

/// Extract the bare project name from various input formats.
/// Strips `compose@` prefix and `.service` suffix if present.
pub fn get_bare_name(service: &str) -> &str {
    let s = service.strip_suffix(".service").unwrap_or(service);
    s.strip_prefix("compose@").unwrap_or(s)
}

/// Get the compose directory for a project.
pub fn get_compose_dir(ctx: &Context, name: &str) -> PathBuf {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);
    ctx.compose_base.join(dir_path)
}

pub fn run_systemctl(ctx: &Context, action: &str, services: &[String]) -> Result<()> {
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg(action);

    for service in services {
        let bare = get_bare_name(service);
        cmd.arg(name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    // If the action was start, stop or restart, show status afterwards
    if action == "start" || action == "stop" || action == "restart" {
        println!("\nService status:");
        let mut status_cmd = Command::new(&ctx.systemctl_cmd[0]);
        if ctx.systemctl_cmd.len() > 1 {
            status_cmd.args(&ctx.systemctl_cmd[1..]);
        }
        status_cmd.args(["status", "-n0", "--no-pager"]);
        for service in services {
            let bare = get_bare_name(service);
            status_cmd.arg(name_to_service(bare));
        }
        let _ = status_cmd.status();
    }

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::core::Context;
    use std::fs;
    use std::path::{Path, PathBuf};

    fn test_context(compose_base: &Path) -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            systemctl_cmd: vec!["systemctl".to_string(), "--user".to_string()],
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
    fn test_name_to_service_simple() {
        assert_eq!(name_to_service("myapp"), "compose@myapp.service");
    }

    #[test]
    fn test_name_to_service_with_dash() {
        assert_eq!(name_to_service("my-app"), "compose@my-app.service");
    }

    #[test]
    fn test_name_to_service_with_slash() {
        assert_eq!(
            name_to_service("genai/ollama"),
            "compose@genai-ollama.service"
        );
    }

    #[test]
    fn test_name_to_service_nested_deep() {
        assert_eq!(name_to_service("a/b/c/d"), "compose@a-b-c-d.service");
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
}
