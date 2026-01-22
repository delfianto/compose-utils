use crate::core::Context;
use anyhow::{Context as _, Result, bail};
use clap::Args;
use regex::Regex;
use std::collections::HashMap;
use std::fs;
use std::net::ToSocketAddrs;
use std::path::Path;
use url::Url;

const CONFIG_KEYS: &[&str] = &[
    "COMPOSE_DATA",
    "COMPOSE_BASE",
    "TRAEFIK_ACME_DOMAIN",
    "TRAEFIK_ACME_EMAIL",
    "TRAEFIK_ACME_SERVER",
    "DOCKER_HOST",
];

#[derive(Args)]
pub struct ConfigArgs {
    #[arg(long, help = "Set COMPOSE_DATA directory path")]
    compose_data: Option<String>,

    #[arg(long, help = "Set COMPOSE_BASE directory path")]
    compose_base: Option<String>,

    #[arg(long, help = "Set ACME domain for Traefik")]
    acme_domain: Option<String>,

    #[arg(long, help = "Set ACME email for Traefik")]
    acme_email: Option<String>,

    #[arg(long, help = "Set ACME server URL for Traefik")]
    acme_server: Option<String>,

    #[arg(long, help = "Set DOCKER_HOST")]
    docker_host: Option<String>,
}

pub fn run(ctx: &Context, args: ConfigArgs) -> Result<()> {
    if args.has_updates() {
        update_config(ctx, args)
    } else {
        show_config(ctx)
    }
}

impl ConfigArgs {
    fn has_updates(&self) -> bool {
        self.compose_data.is_some()
            || self.compose_base.is_some()
            || self.acme_domain.is_some()
            || self.acme_email.is_some()
            || self.acme_server.is_some()
            || self.docker_host.is_some()
    }
}

/// Read the compose.env file and return a map of key-value pairs.
fn read_config(ctx: &Context) -> Result<HashMap<String, String>> {
    let mut config = HashMap::new();

    if !ctx.env_file.exists() {
        return Ok(config);
    }

    let content = fs::read_to_string(&ctx.env_file)
        .with_context(|| format!("Failed to read {}", ctx.env_file.display()))?;

    for line in content.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        if let Some((key, value)) = line.split_once('=') {
            config.insert(key.trim().to_string(), value.trim().to_string());
        }
    }

    Ok(config)
}

/// Write the config back to the compose.env file.
fn write_config(ctx: &Context, config: &HashMap<String, String>) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("# Compose Environment Configuration".to_string());
    lines.push(String::new());

    for key in CONFIG_KEYS {
        if let Some(value) = config.get(*key) {
            lines.push(format!("{}={}", key, value));
        }
    }

    // Preserve any extra keys not in CONFIG_KEYS
    for (key, value) in config {
        if !CONFIG_KEYS.contains(&key.as_str()) {
            lines.push(format!("{}={}", key, value));
        }
    }

    fs::write(&ctx.env_file, lines.join("\n") + "\n")
        .with_context(|| format!("Failed to write {}", ctx.env_file.display()))?;

    Ok(())
}

/// Display the current config in a formatted way.
fn show_config(ctx: &Context) -> Result<()> {
    if !ctx.env_file.exists() {
        println!("Config file not found: {}", ctx.env_file.display());
        println!("Run the installer to generate the config file.");
        return Ok(());
    }

    let config = read_config(ctx)?;

    if config.is_empty() {
        println!("Config file is empty: {}", ctx.env_file.display());
        return Ok(());
    }

    let max_key_len = config.keys().map(|k| k.len()).max().unwrap_or(0);

    println!("Configuration ({})", ctx.env_file.display());
    println!("{}", "-".repeat(60));

    for key in CONFIG_KEYS {
        if let Some(value) = config.get(*key) {
            println!("{:width$}  {}", key, value, width = max_key_len);
        }
    }

    // Show any extra keys
    for (key, value) in &config {
        if !CONFIG_KEYS.contains(&key.as_str()) {
            println!("{:width$}  {}", key, value, width = max_key_len);
        }
    }

    Ok(())
}

/// Update the config with validated values.
fn update_config(ctx: &Context, args: ConfigArgs) -> Result<()> {
    let mut config = read_config(ctx)?;

    if let Some(ref value) = args.compose_data {
        validate_directory(value, "COMPOSE_DATA")?;
        config.insert("COMPOSE_DATA".to_string(), value.clone());
    }

    if let Some(ref value) = args.compose_base {
        validate_directory(value, "COMPOSE_BASE")?;
        config.insert("COMPOSE_BASE".to_string(), value.clone());
    }

    if let Some(ref value) = args.acme_domain {
        validate_domain(value)?;
        config.insert("TRAEFIK_ACME_DOMAIN".to_string(), value.clone());
    }

    if let Some(ref value) = args.acme_email {
        validate_email(value)?;
        config.insert("TRAEFIK_ACME_EMAIL".to_string(), value.clone());
    }

    if let Some(ref value) = args.acme_server {
        validate_acme_server(value)?;
        config.insert("TRAEFIK_ACME_SERVER".to_string(), value.clone());
    }

    if let Some(ref value) = args.docker_host {
        validate_docker_host(value)?;
        config.insert("DOCKER_HOST".to_string(), value.clone());
    }

    write_config(ctx, &config)?;
    println!("Configuration updated successfully.");

    Ok(())
}

/// Validate that a path is an existing directory.
fn validate_directory(path: &str, field_name: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.exists() {
        bail!("{} path does not exist: {}", field_name, path.display());
    }
    if !path.is_dir() {
        bail!("{} path is not a directory: {}", field_name, path.display());
    }
    Ok(())
}

/// Validate that a string resembles a valid domain name.
fn validate_domain(domain: &str) -> Result<()> {
    let domain_regex =
        Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap();

    if !domain_regex.is_match(domain) {
        bail!(
            "TRAEFIK_ACME_DOMAIN must be a valid domain name (e.g., example.com): {}",
            domain
        );
    }
    Ok(())
}

/// Validate that a string is a valid email address.
fn validate_email(email: &str) -> Result<()> {
    let email_regex = Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap();

    if !email_regex.is_match(email) {
        bail!(
            "TRAEFIK_ACME_EMAIL must be a valid email address: {}",
            email
        );
    }
    Ok(())
}

/// Validate that a URL is valid and resolvable.
fn validate_acme_server(server: &str) -> Result<()> {
    let url = Url::parse(server)
        .with_context(|| format!("TRAEFIK_ACME_SERVER must be a valid URL: {}", server))?;

    if url.scheme() != "https" && url.scheme() != "http" {
        bail!(
            "TRAEFIK_ACME_SERVER must use http or https scheme: {}",
            server
        );
    }

    let host = url
        .host_str()
        .ok_or_else(|| anyhow::anyhow!("TRAEFIK_ACME_SERVER URL must have a host: {}", server))?;

    let port = url
        .port()
        .unwrap_or(if url.scheme() == "https" { 443 } else { 80 });
    let addr = format!("{}:{}", host, port);

    addr.to_socket_addrs()
        .with_context(|| format!("TRAEFIK_ACME_SERVER host is not resolvable: {}", host))?;

    Ok(())
}

/// Validate that a DOCKER_HOST value is valid.
fn validate_docker_host(host: &str) -> Result<()> {
    if let Some(socket_path) = host.strip_prefix("unix://") {
        let path = Path::new(socket_path);
        if !path.exists() {
            bail!("DOCKER_HOST unix socket does not exist: {}", socket_path);
        }
        return Ok(());
    }

    if host.starts_with("tcp://") {
        let url = Url::parse(host)
            .with_context(|| format!("DOCKER_HOST tcp URL is invalid: {}", host))?;

        if url.host_str().is_none() {
            bail!("DOCKER_HOST tcp URL must have a host: {}", host);
        }

        return Ok(());
    }

    if host.starts_with("ssh://") {
        let url = Url::parse(host)
            .with_context(|| format!("DOCKER_HOST ssh URL is invalid: {}", host))?;

        if url.host_str().is_none() {
            bail!("DOCKER_HOST ssh URL must have a host: {}", host);
        }

        return Ok(());
    }

    bail!(
        "DOCKER_HOST must be a valid docker endpoint (unix://, tcp://, or ssh://): {}",
        host
    );
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::path::PathBuf;

    struct TestDir {
        base: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let base = PathBuf::from(format!(
                "/tmp/compose-config-test-{}-{}",
                name,
                std::process::id()
            ));
            fs::create_dir_all(&base).unwrap();
            Self { base }
        }

        fn path(&self) -> &Path {
            &self.base
        }

        fn env_file(&self) -> PathBuf {
            self.base.join("compose.env")
        }

        fn context(&self) -> Context {
            Context {
                is_root: false,
                systemd_dir: PathBuf::from("/tmp/test-systemd"),
                systemctl_cmd: vec!["systemctl".to_string(), "--user".to_string()],
                compose_base: self.base.clone(),
                env_file: self.env_file(),
            }
        }

        fn write_env(&self, content: &str) {
            fs::write(self.env_file(), content).unwrap();
        }

        fn read_env(&self) -> String {
            fs::read_to_string(self.env_file()).unwrap()
        }

        fn create_subdir(&self, name: &str) -> PathBuf {
            let dir = self.base.join(name);
            fs::create_dir_all(&dir).unwrap();
            dir
        }
    }

    impl Drop for TestDir {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.base);
        }
    }

    // ConfigArgs tests

    #[test]
    fn test_has_updates_none() {
        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };
        assert!(!args.has_updates());
    }

    #[test]
    fn test_has_updates_compose_data() {
        let args = ConfigArgs {
            compose_data: Some("/tmp".to_string()),
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };
        assert!(args.has_updates());
    }

    #[test]
    fn test_has_updates_compose_base() {
        let args = ConfigArgs {
            compose_data: None,
            compose_base: Some("/tmp".to_string()),
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };
        assert!(args.has_updates());
    }

    #[test]
    fn test_has_updates_traefik_domain() {
        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: Some("example.com".to_string()),
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };
        assert!(args.has_updates());
    }

    #[test]
    fn test_has_updates_multiple() {
        let args = ConfigArgs {
            compose_data: Some("/tmp".to_string()),
            compose_base: Some("/tmp".to_string()),
            acme_domain: Some("example.com".to_string()),
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };
        assert!(args.has_updates());
    }

    // read_config tests

    #[test]
    fn test_read_config_nonexistent_file() {
        let test_dir = TestDir::new("read-nonexistent");
        let ctx = test_dir.context();
        // env_file does not exist
        let config = read_config(&ctx).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_read_config_empty_file() {
        let test_dir = TestDir::new("read-empty");
        test_dir.write_env("");
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_read_config_comments_only() {
        let test_dir = TestDir::new("read-comments");
        test_dir.write_env("# This is a comment\n# Another comment\n");
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert!(config.is_empty());
    }

    #[test]
    fn test_read_config_simple() {
        let test_dir = TestDir::new("read-simple");
        test_dir.write_env("KEY=value\n");
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert_eq!(config.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_read_config_with_spaces() {
        let test_dir = TestDir::new("read-spaces");
        test_dir.write_env("  KEY  =  value  \n");
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert_eq!(config.get("KEY"), Some(&"value".to_string()));
    }

    #[test]
    fn test_read_config_mixed_content() {
        let test_dir = TestDir::new("read-mixed");
        test_dir.write_env(
            "# Header comment\n\
             COMPOSE_DATA=/srv/data\n\
             \n\
             # Another comment\n\
             COMPOSE_BASE=/srv/compose\n\
             TRAEFIK_ACME_DOMAIN=example.com\n",
        );
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert_eq!(config.get("COMPOSE_DATA"), Some(&"/srv/data".to_string()));
        assert_eq!(
            config.get("COMPOSE_BASE"),
            Some(&"/srv/compose".to_string())
        );
        assert_eq!(
            config.get("TRAEFIK_ACME_DOMAIN"),
            Some(&"example.com".to_string())
        );
    }

    #[test]
    fn test_read_config_value_with_equals() {
        let test_dir = TestDir::new("read-equals");
        // Value contains = sign
        test_dir.write_env("URL=https://example.com?foo=bar\n");
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert_eq!(
            config.get("URL"),
            Some(&"https://example.com?foo=bar".to_string())
        );
    }

    #[test]
    fn test_read_config_all_known_keys() {
        let test_dir = TestDir::new("read-all-keys");
        test_dir.write_env(
            "COMPOSE_DATA=/data\n\
             COMPOSE_BASE=/compose\n\
             TRAEFIK_ACME_DOMAIN=example.com\n\
             TRAEFIK_ACME_EMAIL=admin@example.com\n\
             TRAEFIK_ACME_SERVER=https://acme.example.com\n\
             DOCKER_HOST=unix:///var/run/docker.sock\n",
        );
        let ctx = test_dir.context();
        let config = read_config(&ctx).unwrap();
        assert_eq!(config.len(), 6);
        assert_eq!(config.get("COMPOSE_DATA"), Some(&"/data".to_string()));
        assert_eq!(config.get("COMPOSE_BASE"), Some(&"/compose".to_string()));
        assert_eq!(
            config.get("TRAEFIK_ACME_DOMAIN"),
            Some(&"example.com".to_string())
        );
        assert_eq!(
            config.get("TRAEFIK_ACME_EMAIL"),
            Some(&"admin@example.com".to_string())
        );
        assert_eq!(
            config.get("TRAEFIK_ACME_SERVER"),
            Some(&"https://acme.example.com".to_string())
        );
        assert_eq!(
            config.get("DOCKER_HOST"),
            Some(&"unix:///var/run/docker.sock".to_string())
        );
    }

    // write_config tests

    #[test]
    fn test_write_config_empty() {
        let test_dir = TestDir::new("write-empty");
        let ctx = test_dir.context();
        let config = HashMap::new();
        write_config(&ctx, &config).unwrap();
        let content = test_dir.read_env();
        assert!(content.contains("# Compose Environment Configuration"));
    }

    #[test]
    fn test_write_config_single_key() {
        let test_dir = TestDir::new("write-single");
        let ctx = test_dir.context();
        let mut config = HashMap::new();
        config.insert("COMPOSE_DATA".to_string(), "/srv/data".to_string());
        write_config(&ctx, &config).unwrap();
        let content = test_dir.read_env();
        assert!(content.contains("COMPOSE_DATA=/srv/data"));
    }

    #[test]
    fn test_write_config_preserves_order() {
        let test_dir = TestDir::new("write-order");
        let ctx = test_dir.context();
        let mut config = HashMap::new();
        config.insert("COMPOSE_DATA".to_string(), "/data".to_string());
        config.insert("COMPOSE_BASE".to_string(), "/base".to_string());
        config.insert("TRAEFIK_ACME_DOMAIN".to_string(), "example.com".to_string());
        write_config(&ctx, &config).unwrap();
        let content = test_dir.read_env();
        // Check that keys appear in the expected order
        let data_pos = content.find("COMPOSE_DATA").unwrap();
        let base_pos = content.find("COMPOSE_BASE").unwrap();
        let domain_pos = content.find("TRAEFIK_ACME_DOMAIN").unwrap();
        assert!(data_pos < base_pos);
        assert!(base_pos < domain_pos);
    }

    #[test]
    fn test_write_config_preserves_extra_keys() {
        let test_dir = TestDir::new("write-extra");
        let ctx = test_dir.context();
        let mut config = HashMap::new();
        config.insert("COMPOSE_DATA".to_string(), "/data".to_string());
        config.insert("CUSTOM_KEY".to_string(), "custom_value".to_string());
        write_config(&ctx, &config).unwrap();
        let content = test_dir.read_env();
        assert!(content.contains("COMPOSE_DATA=/data"));
        assert!(content.contains("CUSTOM_KEY=custom_value"));
    }

    // roundtrip tests

    #[test]
    fn test_read_write_roundtrip() {
        let test_dir = TestDir::new("roundtrip");
        let ctx = test_dir.context();

        let mut original = HashMap::new();
        original.insert("COMPOSE_DATA".to_string(), "/data".to_string());
        original.insert("COMPOSE_BASE".to_string(), "/base".to_string());
        original.insert("TRAEFIK_ACME_DOMAIN".to_string(), "example.com".to_string());

        write_config(&ctx, &original).unwrap();
        let read_back = read_config(&ctx).unwrap();

        assert_eq!(read_back.get("COMPOSE_DATA"), original.get("COMPOSE_DATA"));
        assert_eq!(read_back.get("COMPOSE_BASE"), original.get("COMPOSE_BASE"));
        assert_eq!(
            read_back.get("TRAEFIK_ACME_DOMAIN"),
            original.get("TRAEFIK_ACME_DOMAIN")
        );
    }

    #[test]
    fn test_update_preserves_existing() {
        let test_dir = TestDir::new("update-preserve");
        test_dir.write_env(
            "COMPOSE_DATA=/original\n\
             COMPOSE_BASE=/base\n",
        );
        let ctx = test_dir.context();

        let mut config = read_config(&ctx).unwrap();
        config.insert("COMPOSE_DATA".to_string(), "/updated".to_string());
        write_config(&ctx, &config).unwrap();

        let final_config = read_config(&ctx).unwrap();
        assert_eq!(
            final_config.get("COMPOSE_DATA"),
            Some(&"/updated".to_string())
        );
        assert_eq!(final_config.get("COMPOSE_BASE"), Some(&"/base".to_string()));
    }

    // Domain validation tests

    #[test]
    fn test_validate_domain_simple() {
        assert!(validate_domain("example.com").is_ok());
    }

    #[test]
    fn test_validate_domain_subdomain() {
        assert!(validate_domain("sub.example.com").is_ok());
        assert!(validate_domain("deep.sub.example.com").is_ok());
    }

    #[test]
    fn test_validate_domain_with_hyphen() {
        assert!(validate_domain("my-domain.com").is_ok());
        assert!(validate_domain("my-cool-domain.co.uk").is_ok());
    }

    #[test]
    fn test_validate_domain_with_numbers() {
        assert!(validate_domain("domain123.com").is_ok());
        assert!(validate_domain("123domain.com").is_ok());
    }

    #[test]
    fn test_validate_domain_various_tlds() {
        assert!(validate_domain("example.io").is_ok());
        assert!(validate_domain("example.co.uk").is_ok());
        assert!(validate_domain("example.technology").is_ok());
    }

    #[test]
    fn test_validate_domain_invalid_no_tld() {
        assert!(validate_domain("example").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_starts_with_hyphen() {
        assert!(validate_domain("-example.com").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_ends_with_hyphen() {
        assert!(validate_domain("example-.com").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_spaces() {
        assert!(validate_domain("example .com").is_err());
        assert!(validate_domain("example. com").is_err());
        assert!(validate_domain(" example.com").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_special_chars() {
        assert!(validate_domain("example_domain.com").is_err());
        assert!(validate_domain("example@domain.com").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_empty() {
        assert!(validate_domain("").is_err());
    }

    #[test]
    fn test_validate_domain_invalid_just_dot() {
        assert!(validate_domain(".").is_err());
        assert!(validate_domain("..").is_err());
    }

    // Email validation tests

    #[test]
    fn test_validate_email_simple() {
        assert!(validate_email("user@example.com").is_ok());
    }

    #[test]
    fn test_validate_email_with_dots() {
        assert!(validate_email("user.name@example.com").is_ok());
        assert!(validate_email("first.middle.last@example.com").is_ok());
    }

    #[test]
    fn test_validate_email_with_plus() {
        assert!(validate_email("user+tag@example.com").is_ok());
        assert!(validate_email("user+tag+another@example.com").is_ok());
    }

    #[test]
    fn test_validate_email_with_numbers() {
        assert!(validate_email("user123@example.com").is_ok());
        assert!(validate_email("123user@example.com").is_ok());
    }

    #[test]
    fn test_validate_email_subdomain() {
        assert!(validate_email("user@mail.example.com").is_ok());
        assert!(validate_email("user@sub.mail.example.co.uk").is_ok());
    }

    #[test]
    fn test_validate_email_invalid_no_at() {
        assert!(validate_email("userexample.com").is_err());
    }

    #[test]
    fn test_validate_email_invalid_no_domain() {
        assert!(validate_email("user@").is_err());
    }

    #[test]
    fn test_validate_email_invalid_no_user() {
        assert!(validate_email("@example.com").is_err());
    }

    #[test]
    fn test_validate_email_invalid_no_tld() {
        assert!(validate_email("user@example").is_err());
    }

    #[test]
    fn test_validate_email_invalid_spaces() {
        assert!(validate_email("user @example.com").is_err());
        assert!(validate_email("user@ example.com").is_err());
        assert!(validate_email(" user@example.com").is_err());
    }

    #[test]
    fn test_validate_email_invalid_empty() {
        assert!(validate_email("").is_err());
    }

    #[test]
    fn test_validate_email_invalid_multiple_at() {
        assert!(validate_email("user@@example.com").is_err());
    }

    // ACME server validation tests

    #[test]
    fn test_validate_acme_server_https() {
        // Using a real resolvable domain
        assert!(validate_acme_server("https://acme-v02.api.letsencrypt.org/directory").is_ok());
    }

    #[test]
    fn test_validate_acme_server_http() {
        // HTTP is allowed (for staging servers)
        assert!(validate_acme_server("http://localhost:8080/directory").is_ok());
    }

    #[test]
    fn test_validate_acme_server_invalid_scheme() {
        assert!(validate_acme_server("ftp://example.com").is_err());
        assert!(validate_acme_server("file:///etc/passwd").is_err());
    }

    #[test]
    fn test_validate_acme_server_invalid_not_url() {
        assert!(validate_acme_server("not a url").is_err());
        assert!(validate_acme_server("example.com").is_err());
    }

    #[test]
    fn test_validate_acme_server_unresolvable() {
        assert!(validate_acme_server("https://this-domain-does-not-exist-12345.invalid").is_err());
    }

    // Docker host validation tests

    #[test]
    fn test_validate_docker_host_unix_exists() {
        let test_dir = TestDir::new("docker-unix");
        let socket = test_dir.path().join("docker.sock");
        fs::write(&socket, "").unwrap();
        let host = format!("unix://{}", socket.display());
        assert!(validate_docker_host(&host).is_ok());
    }

    #[test]
    fn test_validate_docker_host_unix_not_exists() {
        assert!(validate_docker_host("unix:///nonexistent/docker.sock").is_err());
    }

    #[test]
    fn test_validate_docker_host_tcp_localhost() {
        assert!(validate_docker_host("tcp://localhost:2375").is_ok());
    }

    #[test]
    fn test_validate_docker_host_tcp_ip() {
        assert!(validate_docker_host("tcp://192.168.1.1:2376").is_ok());
        assert!(validate_docker_host("tcp://10.0.0.1:2375").is_ok());
    }

    #[test]
    fn test_validate_docker_host_tcp_hostname() {
        assert!(validate_docker_host("tcp://docker.example.com:2376").is_ok());
    }

    #[test]
    fn test_validate_docker_host_tcp_no_port() {
        assert!(validate_docker_host("tcp://localhost").is_ok());
    }

    #[test]
    fn test_validate_docker_host_ssh_simple() {
        assert!(validate_docker_host("ssh://user@host").is_ok());
    }

    #[test]
    fn test_validate_docker_host_ssh_with_port() {
        assert!(validate_docker_host("ssh://user@host:22").is_ok());
    }

    #[test]
    fn test_validate_docker_host_ssh_no_user() {
        assert!(validate_docker_host("ssh://host").is_ok());
    }

    #[test]
    fn test_validate_docker_host_invalid_http() {
        assert!(validate_docker_host("http://localhost").is_err());
    }

    #[test]
    fn test_validate_docker_host_invalid_https() {
        assert!(validate_docker_host("https://localhost").is_err());
    }

    #[test]
    fn test_validate_docker_host_invalid_bare_path() {
        assert!(validate_docker_host("/var/run/docker.sock").is_err());
    }

    #[test]
    fn test_validate_docker_host_invalid_empty() {
        assert!(validate_docker_host("").is_err());
    }

    #[test]
    fn test_validate_docker_host_invalid_random() {
        assert!(validate_docker_host("random").is_err());
        assert!(validate_docker_host("localhost:2375").is_err());
    }

    // Directory validation tests

    #[test]
    fn test_validate_directory_tmp() {
        assert!(validate_directory("/tmp", "TEST").is_ok());
    }

    #[test]
    fn test_validate_directory_created() {
        let test_dir = TestDir::new("dir-validate");
        let subdir = test_dir.create_subdir("mydir");
        assert!(validate_directory(subdir.to_str().unwrap(), "TEST").is_ok());
    }

    #[test]
    fn test_validate_directory_nonexistent() {
        assert!(validate_directory("/nonexistent/path/12345", "TEST").is_err());
    }

    #[test]
    fn test_validate_directory_is_file() {
        let test_dir = TestDir::new("dir-is-file");
        let file_path = test_dir.path().join("file.txt");
        fs::write(&file_path, "content").unwrap();
        assert!(validate_directory(file_path.to_str().unwrap(), "TEST").is_err());
    }

    #[test]
    fn test_validate_directory_error_message_contains_field_name() {
        let result = validate_directory("/nonexistent", "MY_FIELD");
        assert!(result.is_err());
        let err = result.unwrap_err().to_string();
        assert!(err.contains("MY_FIELD"));
    }

    // Integration tests

    #[test]
    fn test_update_single_value_integration() {
        let test_dir = TestDir::new("update-single");
        let subdir = test_dir.create_subdir("data");
        test_dir.write_env("COMPOSE_DATA=/old\nCOMPOSE_BASE=/base\n");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: Some(subdir.to_str().unwrap().to_string()),
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };

        let result = update_config(&ctx, args);
        assert!(result.is_ok());

        let config = read_config(&ctx).unwrap();
        assert_eq!(
            config.get("COMPOSE_DATA"),
            Some(&subdir.to_str().unwrap().to_string())
        );
        assert_eq!(config.get("COMPOSE_BASE"), Some(&"/base".to_string()));
    }

    #[test]
    fn test_update_multiple_values_integration() {
        let test_dir = TestDir::new("update-multi");
        let data_dir = test_dir.create_subdir("data");
        let base_dir = test_dir.create_subdir("base");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: Some(data_dir.to_str().unwrap().to_string()),
            compose_base: Some(base_dir.to_str().unwrap().to_string()),
            acme_domain: Some("example.com".to_string()),
            acme_email: Some("admin@example.com".to_string()),
            acme_server: None,
            docker_host: None,
        };

        let result = update_config(&ctx, args);
        assert!(result.is_ok());

        let config = read_config(&ctx).unwrap();
        assert_eq!(
            config.get("COMPOSE_DATA"),
            Some(&data_dir.to_str().unwrap().to_string())
        );
        assert_eq!(
            config.get("COMPOSE_BASE"),
            Some(&base_dir.to_str().unwrap().to_string())
        );
        assert_eq!(
            config.get("TRAEFIK_ACME_DOMAIN"),
            Some(&"example.com".to_string())
        );
        assert_eq!(
            config.get("TRAEFIK_ACME_EMAIL"),
            Some(&"admin@example.com".to_string())
        );
    }

    #[test]
    fn test_update_fails_on_invalid_directory() {
        let test_dir = TestDir::new("update-invalid-dir");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: Some("/nonexistent/path".to_string()),
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };

        let result = update_config(&ctx, args);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_fails_on_invalid_domain() {
        let test_dir = TestDir::new("update-invalid-domain");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: Some("not a domain".to_string()),
            acme_email: None,
            acme_server: None,
            docker_host: None,
        };

        let result = update_config(&ctx, args);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_fails_on_invalid_email() {
        let test_dir = TestDir::new("update-invalid-email");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: None,
            acme_email: Some("not an email".to_string()),
            acme_server: None,
            docker_host: None,
        };

        let result = update_config(&ctx, args);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_fails_on_invalid_docker_host() {
        let test_dir = TestDir::new("update-invalid-docker");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: Some("invalid".to_string()),
        };

        let result = update_config(&ctx, args);
        assert!(result.is_err());
    }

    #[test]
    fn test_update_with_docker_host_tcp() {
        let test_dir = TestDir::new("update-docker-tcp");
        test_dir.write_env("");
        let ctx = test_dir.context();

        let args = ConfigArgs {
            compose_data: None,
            compose_base: None,
            acme_domain: None,
            acme_email: None,
            acme_server: None,
            docker_host: Some("tcp://localhost:2375".to_string()),
        };

        let result = update_config(&ctx, args);
        assert!(result.is_ok());

        let config = read_config(&ctx).unwrap();
        assert_eq!(
            config.get("DOCKER_HOST"),
            Some(&"tcp://localhost:2375".to_string())
        );
    }
}
