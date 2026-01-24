use crate::constants::COMPOSE_FILES;
use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use colored::*;
use regex::Regex;
use serde::Deserialize;
use std::collections::HashMap;
use std::env;
use std::fs;
use std::os::unix::fs::symlink;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

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

/// Struct representing a subset of docker-compose.yml structure for image parsing
#[derive(Deserialize, Debug)]
struct DockerCompose {
    services: Option<HashMap<String, ComposeService>>,
}

#[derive(Deserialize, Debug)]
struct ComposeService {
    image: Option<String>,
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

/// Pull images for services without restarting
pub fn run_pull(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;
    validate_compose_dirs(ctx, &services)?;

    for service in &services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);

        println!("{} Pulling images for '{}'...", ">>".blue(), bare);

        let images = get_images_for_project(&dir)?;
        if images.is_empty() {
            println!("No images defined in compose file for '{}'", bare);
            continue;
        }

        pull_images(&images)?;
    }

    println!("{} All images pulled successfully.", "OK".green());
    Ok(())
}

/// Update services: pull images, detect changes, restart only if needed
pub fn run_update(ctx: &Context, services: &[String]) -> Result<()> {
    let services = resolve_services(ctx, services)?;
    validate_compose_dirs(ctx, &services)?;

    let mut services_to_restart = Vec::new();

    for service in &services {
        let bare = get_bare_name(service);
        let dir = get_compose_dir(ctx, bare);

        println!("{} Checking for updates: '{}'...", ">>".blue(), bare);

        let images = get_images_for_project(&dir)?;
        if images.is_empty() {
            println!("No images defined in compose file for '{}'", bare);
            continue;
        }

        // Capture current image states
        let mut pre_pull_hashes = HashMap::new();
        for image in &images {
            let hash = get_image_digest(image);
            pre_pull_hashes.insert(image.clone(), hash);
        }

        // Pull images
        pull_images(&images)?;

        // Compare states
        let mut updated = false;
        for image in &images {
            let old_hash = pre_pull_hashes.get(image).unwrap_or(&None);
            let new_hash = get_image_digest(image);

            match (old_hash, &new_hash) {
                (Some(old), Some(new)) if old != new => {
                    println!(
                        "{} Image updated: {} ({} -> {})",
                        "+".green(),
                        image,
                        shorten_hash(old),
                        shorten_hash(new)
                    );
                    updated = true;
                }
                (None, Some(new)) => {
                    println!(
                        "{} New image downloaded: {} ({})",
                        "+".green(),
                        image,
                        shorten_hash(new)
                    );
                    updated = true;
                }
                _ => {
                    // No change
                }
            }
        }

        if updated {
            services_to_restart.push(service.clone());
        } else {
            println!("{} '{}' is already up to date.", "OK".green(), bare);
        }
    }

    // Restart only services that had updates
    if !services_to_restart.is_empty() {
        println!("\nRestarting updated services...");
        run_systemctl(ctx, "restart", &services_to_restart, false)?;
        println!(
            "{} Updated and restarted {} service(s).",
            "OK".green(),
            services_to_restart.len()
        );
    } else {
        println!("\n{} All services are already up to date.", "OK".green());
    }

    Ok(())
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
        return Ok(());
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

        let row = vec![
            c.id,
            c.names,
            c.image,
            iso_created,
            c.state,
            c.status,
            c.ports,
        ];

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
        return Ok(());
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
            return Ok(());
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

/// Load environment variables from a .env file
fn load_env_file(path: &Path) -> Result<HashMap<String, String>> {
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

/// Resolve environment variables in a string (e.g., ${TAG} or $TAG)
fn resolve_env_vars(text: &str, vars: &HashMap<String, String>) -> String {
    let re = Regex::new(r"\$\{?([a-zA-Z_][a-zA-Z0-9_]*)\}?").unwrap();
    re.replace_all(text, |caps: &regex::Captures| {
        let key = &caps[1];
        vars.get(key)
            .cloned()
            .unwrap_or_else(|| caps[0].to_string())
    })
    .to_string()
}

/// Find compose file in a directory
fn find_compose_file(dir: &Path) -> Option<PathBuf> {
    for name in COMPOSE_FILES {
        let path = dir.join(name);
        if path.exists() {
            return Some(path);
        }
    }
    None
}

/// Get images from a compose project directory
fn get_images_for_project(project_dir: &Path) -> Result<Vec<String>> {
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

    let compose: DockerCompose = serde_yaml::from_str(&content)
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

/// Get the digest/ID of a local docker image
fn get_image_digest(image: &str) -> Option<String> {
    let output = Command::new("docker")
        .arg("inspect")
        .arg("--format={{.Id}}")
        .arg(image)
        .stdout(Stdio::piped())
        .stderr(Stdio::null())
        .output()
        .ok()?;

    if output.status.success() {
        let s = String::from_utf8_lossy(&output.stdout).trim().to_string();
        if s.is_empty() { None } else { Some(s) }
    } else {
        None
    }
}

/// Pull docker images
fn pull_images(images: &[String]) -> Result<()> {
    for image in images {
        println!("Pulling {}...", image);
        let status = Command::new("docker")
            .arg("pull")
            .arg(image)
            .stdout(Stdio::inherit())
            .stderr(Stdio::inherit())
            .status()
            .with_context(|| format!("Failed to run docker pull {}", image))?;

        if !status.success() {
            eprintln!("{} Failed to pull {}", "Warning:".yellow(), image);
        }
        println!();
    }
    Ok(())
}

/// Shorten a hash for display
fn shorten_hash(hash: &str) -> String {
    if hash.len() > 12 {
        hash[..12].to_string()
    } else {
        hash.to_string()
    }
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

    // Tests for new image parsing and env var substitution

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
        writeln!(file, "").unwrap();
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

    #[test]
    fn test_parse_compose_images() {
        let yaml = r#"
services:
  web:
    image: nginx:${TAG}
  db:
    image: postgres:14
"#;

        let compose: DockerCompose = serde_yaml::from_str(yaml).unwrap();
        let services = compose.services.unwrap();
        assert_eq!(
            services.get("web").unwrap().image,
            Some("nginx:${TAG}".to_string())
        );
        assert_eq!(
            services.get("db").unwrap().image,
            Some("postgres:14".to_string())
        );
    }

    #[test]
    fn test_parse_compose_no_image() {
        // Some services might use build instead of image
        let yaml = r#"
services:
  app:
    build: .
  db:
    image: postgres:14
"#;

        let compose: DockerCompose = serde_yaml::from_str(yaml).unwrap();
        let services = compose.services.unwrap();
        assert_eq!(services.get("app").unwrap().image, None);
        assert_eq!(
            services.get("db").unwrap().image,
            Some("postgres:14".to_string())
        );
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

    #[test]
    fn test_shorten_hash() {
        assert_eq!(shorten_hash("sha256:abc123def456xyz789"), "sha256:abc12");
        assert_eq!(shorten_hash("short"), "short");
    }

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
}
