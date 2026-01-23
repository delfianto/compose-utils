use crate::constants::COMPOSE_FILES;
use crate::core::Context;
use anyhow::{bail, Context as _, Result};
use serde::Deserialize;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::PathBuf;
use std::process::Command;

#[derive(Deserialize, Debug)]
#[serde(rename_all = "PascalCase")]
struct DockerContainer {
    #[serde(rename = "ID")]
    id: String,
    names: String,
    image: String,
    created_at: String,
    state: String,
    status: String,
    ports: String,
}

/// Convert project name to directory path.
/// Handles both flat names (test-project) and nested names (genai-ollama -> genai/ollama).
fn name_to_dir_path(ctx: &Context, name: &str) -> String {
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
fn name_to_service(name: &str) -> String {
    let normalized = name.replace('/', "-");
    format!("compose@{}.service", normalized)
}

/// Extract the bare project name from various input formats.
/// Strips `compose@` prefix and `.service` suffix if present.
fn get_bare_name(service: &str) -> &str {
    let s = service.strip_suffix(".service").unwrap_or(service);
    s.strip_prefix("compose@").unwrap_or(s)
}

/// Get the compose directory for a project.
fn get_compose_dir(ctx: &Context, name: &str) -> PathBuf {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);
    ctx.compose_base.join(dir_path)
}

/// Detect service name from current directory.
/// Returns Some(service_name) if current directory is under compose_base
/// and contains a compose file.
fn detect_service_from_cwd(ctx: &Context) -> Option<String> {
    let cwd = env::current_dir().ok()?;
    let rel_path = cwd.strip_prefix(&ctx.compose_base).ok()?;

    let has_compose_file = COMPOSE_FILES.iter().any(|f| cwd.join(f).exists());
    if !has_compose_file {
        return None;
    }

    let service_name = rel_path
        .to_str()?
        .replace([std::path::MAIN_SEPARATOR, '/'], "-");

    Some(service_name)
}

/// Resolve services list. If empty, try to detect from current directory.
fn resolve_services(ctx: &Context, services: &[String]) -> Result<Vec<String>> {
    if !services.is_empty() {
        return Ok(services.to_vec());
    }

    if let Some(service) = detect_service_from_cwd(ctx) {
        println!("Auto-detected service: {}", service);
        return Ok(vec![service]);
    }

    bail!(
        "No service specified and current directory is not a compose project.\n\
        Either specify a service name or run from a directory under {}",
        ctx.compose_base.display()
    );
}

/// Resolve a single service. If empty, try to detect from current directory.
fn resolve_service(ctx: &Context, service: &str) -> Result<String> {
    if !service.is_empty() {
        return Ok(service.to_string());
    }

    if let Some(detected) = detect_service_from_cwd(ctx) {
        println!("Auto-detected service: {}", detected);
        return Ok(detected);
    }

    bail!(
        "No service specified and current directory is not a compose project.\n\
        Either specify a service name or run from a directory under {}",
        ctx.compose_base.display()
    );
}

/// Validate that compose directories exist for all given services.
fn validate_compose_dirs(ctx: &Context, services: &[String]) -> Result<()> {
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

/// Get all managed services by scanning COMPOSE_BASE.
fn find_all_services(ctx: &Context) -> Result<Vec<String>> {
    let mut services = Vec::new();
    if !ctx.compose_base.exists() {
        return Ok(services);
    }

    fn scan_dir(base: &std::path::Path, current: PathBuf, services: &mut Vec<String>) {
        let entries = match fs::read_dir(&current) {
            Ok(e) => e,
            Err(_) => return,
        };

        let mut is_project = false;
        let mut subdirs = Vec::new();

        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                subdirs.push(path);
            } else if path.is_file() {
                if let Some(name) = path.file_name().and_then(|n| n.to_str()) {
                    if COMPOSE_FILES.contains(&name) {
                        is_project = true;
                    }
                }
            }
        }

        if is_project {
            if let Ok(rel_path) = current.strip_prefix(base) {
                if let Some(s) = rel_path.to_str() {
                    if !s.is_empty() {
                        services.push(s.replace([std::path::MAIN_SEPARATOR, '/'], "-"));
                    }
                }
            }
        }

        // Continue searching subdirectories regardless of whether this dir is a project
        for subdir in subdirs {
            scan_dir(base, subdir, services);
        }
    }

    scan_dir(&ctx.compose_base, ctx.compose_base.clone(), &mut services);
    services.sort();
    services.dedup();
    Ok(services)
}

pub fn run_systemctl(
    ctx: &Context,
    action: &str,
    services: &[String],
    validate: bool,
) -> Result<()> {
    let services = if services.is_empty() && action == "status" {
        if let Some(service) = detect_service_from_cwd(ctx) {
            vec![service]
        } else {
            find_all_services(ctx)?
        }
    } else {
        resolve_services(ctx, services)?
    };

    if services.is_empty() && action == "status" {
        println!("No services found.");
        return Ok(());
    }

    if validate {
        validate_compose_dirs(ctx, &services)?;
    }

    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg(action);

    for service in &services {
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
        for service in &services {
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

pub fn run_start(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "start", services, true)
}

pub fn run_stop(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "stop", services, true)
}

pub fn run_restart(ctx: &Context, services: &[String]) -> Result<()> {
    run_systemctl(ctx, "restart", services, true)
}

pub fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;
    validate_compose_dirs(ctx, &services)?;

    for service in &services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);

        println!("Pulling images for '{}'...", bare);
        let mut pull_cmd = Command::new("docker");
        pull_cmd.args(["compose", "pull"]);
        pull_cmd.current_dir(&dir);

        println!("Running: {:?}", pull_cmd);
        let status = pull_cmd
            .status()
            .context("Failed to run docker compose pull")?;

        if !status.success() {
            bail!(
                "Failed to pull images for '{}' (exit code: {})",
                bare,
                status.code().unwrap_or(1)
            );
        }
    }

    println!("\nRestarting services...");
    run_systemctl(ctx, "restart", &services, false)
}

pub fn run_list(ctx: &Context) -> Result<()> {
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.args(["list-units", "compose@*.service", "--all"]);

    cmd.status().context("Failed to list units")?;
    Ok(())
}

pub fn run_ps(_ctx: &Context, _services: &[String]) -> Result<()> {
    let mut cmd = Command::new("docker");
    cmd.args(["ps", "--all", "--format", "{{json .}}"]);

    let output = cmd.output().context("Failed to run docker ps")?;
    if !output.status.success() {
        bail!(
            "Error running docker ps: {}",
            String::from_utf8_lossy(&output.stderr)
        );
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let c: DockerContainer = 
            serde_json::from_str(line).context("Failed to parse docker ps JSON output")?;
        containers.push(c);
    }

    if containers.is_empty() {
        println!("No containers found.");
        return Ok(())
    }

    let headers = vec![
        "ID",
        "NAME",
        "IMAGE/TAG",
        "CREATED",
        "STATE",
        "STATUS",
        "PORTS",
    ];
    let mut widths = headers.iter().map(|h| h.len()).collect::<Vec<_>>();
    let mut data = Vec::new();

    for c in containers {
        // Handle "2026-01-23 22:30:00 +0700 WIB" or "2026-01-23 22:30:00 +0700" or already ISO
        let iso_created = if c.created_at.contains(' ') {
            let parts: Vec<&str> = c.created_at.split_whitespace().collect();
            if parts.len() >= 3 {
                // "2026-01-23", "22:30:00", "+0700" -> "2026-01-23T22:30:00+0700"
                format!("{}T{}{}", parts[0], parts[1], parts[2])
            } else if parts.len() == 2 {
                // "2026-01-23", "22:30:00" -> "2026-01-23T22:30:00"
                format!("{}T{}", parts[0], parts[1])
            } else {
                c.created_at.replace(' ', "T")
            }
        } else {
            c.created_at
        };

        let row = vec![c.id, c.names, c.image, iso_created, c.state, c.status, c.ports];

        for (i, val) in row.iter().enumerate() {
            if val.len() > widths[i] {
                widths[i] = val.len();
            }
        }
        data.push(row);
    }

    // Print header
    for (i, h) in headers.iter().enumerate() {
        print!("{:<width$}  ", h, width = widths[i]);
    }
    println!();

    // Print rows
    for row in data {
        for (i, val) in row.iter().enumerate() {
            print!("{:<width$}  ", val, width = widths[i]);
        }
        println!();
    }

    Ok(())
}

pub fn run_logs(ctx: &Context, service: &str, follow: bool, lines: Option<usize>) -> Result<()> {
    let service = resolve_service(ctx, service)?;

    let mut cmd = Command::new("journalctl");
    if !ctx.is_root {
        cmd.arg("--user");
    }

    let bare = get_bare_name(&service);
    cmd.arg("-u").arg(name_to_service(bare));

    if follow {
        cmd.arg("-f");
    } else {
        cmd.arg("-e");
    }

    let n = lines.unwrap_or(100);
    cmd.arg("-n").arg(n.to_string());

    println!("Running: {:?}", cmd);
    cmd.status().context("Failed to run logs")?;
    Ok(())
}

/// Get the symlink path for a service (used when directory is nested).
/// Returns the path where a symlink should be created to map the flat name
/// to the nested directory structure.
fn get_symlink_path(ctx: &Context, name: &str) -> Option<PathBuf> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    // If the directory path contains a slash, it's nested
    // and we need a symlink from the flat name to the nested path
    if dir_path.contains('/') {
        let flat_name = bare.replace('/', "-");
        Some(ctx.compose_base.join(flat_name))
    } else {
        None
    }
}

/// Create a symlink for nested directories so systemd can find them.
fn ensure_symlink(ctx: &Context, name: &str) -> Result<()> {
    let bare = get_bare_name(name);
    let dir_path = name_to_dir_path(ctx, bare);

    // Only needed for nested directories
    if !dir_path.contains('/') {
        return Ok(())
    }

    let flat_name = bare.replace('/', "-");
    let symlink_path = ctx.compose_base.join(&flat_name);
    let target_path = ctx.compose_base.join(&dir_path);

    // If symlink already exists and points to the right place, we're done
    if symlink_path.is_symlink() {
        let current_target = fs::read_link(&symlink_path)?;
        if current_target == target_path
            || current_target.as_path() == std::path::Path::new(&dir_path)
        {
            return Ok(())
        }
        // Wrong target, remove and recreate
        fs::remove_file(&symlink_path)?;
    } else if symlink_path.exists() {
        // Something else exists at this path
        bail!(
            "Cannot create symlink at {}: path already exists and is not a symlink",
            symlink_path.display()
        );
    }

    println!(
        "Creating symlink: {} -> {}",
        symlink_path.display(),
        dir_path
    );
    symlink(&dir_path, &symlink_path).with_context(|| {
        format!(
            "Failed to create symlink {} -> {}",
            symlink_path.display(),
            dir_path
        )
    })?;

    Ok(())
}

/// Remove symlink for a service if it exists.
fn remove_symlink(ctx: &Context, name: &str) -> Result<()> {
    if let Some(symlink_path) = get_symlink_path(ctx, name)
        && symlink_path.is_symlink()
    {
        println!("Removing symlink: {}", symlink_path.display());
        fs::remove_file(&symlink_path)?;
    }
    Ok(())
}

pub fn run_enable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;
    validate_compose_dirs(ctx, &services)?;

    // Create symlinks for nested directories
    for service in &services {
        ensure_symlink(ctx, service)?;
    }

    // Run systemctl enable
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("enable");

    for service in &services {
        let bare = get_bare_name(service);
        cmd.arg(name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

pub fn run_disable(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;

    // Run systemctl disable first
    let mut cmd = Command::new(&ctx.systemctl_cmd[0]);
    if ctx.systemctl_cmd.len() > 1 {
        cmd.args(&ctx.systemctl_cmd[1..]);
    }
    cmd.arg("disable");

    for service in &services {
        let bare = get_bare_name(service);
        cmd.arg(name_to_service(bare));
    }

    println!("Running: {:?}", cmd);
    let status = cmd.status().context("Failed to execute systemctl")?;

    // Remove symlinks
    for service in &services {
        remove_symlink(ctx, service)?;
    }

    if !status.success() {
        std::process::exit(status.code().unwrap_or(1));
    }
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::fs;
    use std::path::Path;

    fn test_context(compose_base: &Path) -> Context {
        Context {
            is_root: false,
            systemd_dir: PathBuf::from("/tmp/test-systemd"),
            systemctl_cmd: vec!["systemctl".to_string(), "--user".to_string()],
            compose_base: compose_base.to_path_buf(),
            env_file: PathBuf::from("/tmp/test-compose.env"),
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
        // cwd is not under compose_base, should return None
        // (current test dir is /srv/project/compose-utils, not under /tmp/...)
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
        // myapp -> compose@myapp.service, directory myapp
        let test_dir = TestDir::new("roundtrip-simple");
        test_dir.create_dir("myapp");
        let ctx = test_context(test_dir.path());

        let service_name = name_to_service("myapp");
        assert_eq!(service_name, "compose@myapp.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "myapp");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("myapp"));
    }

    #[test]
    fn test_roundtrip_nested_name() {
        // genai/ollama -> compose@genai-ollama.service, directory genai/ollama
        let test_dir = TestDir::new("roundtrip-nested");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());

        let service_name = name_to_service("genai/ollama");
        assert_eq!(service_name, "compose@genai-ollama.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "genai-ollama");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("genai/ollama"));
    }

    #[test]
    fn test_roundtrip_dash_input_to_nested() {
        // genai-ollama -> compose@genai-ollama.service, directory genai/ollama
        let test_dir = TestDir::new("roundtrip-dash");
        test_dir.create_dir("genai/ollama");
        let ctx = test_context(test_dir.path());

        let service_name = name_to_service("genai-ollama");
        assert_eq!(service_name, "compose@genai-ollama.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "genai-ollama");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("genai/ollama"));
    }

    #[test]
    fn test_roundtrip_flat_with_dash() {
        // my-project (flat dir) -> compose@my-project.service, directory my-project
        let test_dir = TestDir::new("roundtrip-flat-dash");
        test_dir.create_dir("my-project");
        let ctx = test_context(test_dir.path());

        let service_name = name_to_service("my-project");
        assert_eq!(service_name, "compose@my-project.service");

        let bare = get_bare_name(&service_name);
        assert_eq!(bare, "my-project");

        let dir = get_compose_dir(&ctx, bare);
        assert_eq!(dir, test_dir.path().join("my-project"));
    }
}