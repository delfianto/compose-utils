//! Logic for viewing and updating the tool's configuration (`compose.env`).

use crate::core::{
    read_env_file, validate_acme_server, validate_directory, validate_docker_host, validate_domain,
    validate_email, Context, CONFIG_KEYS,
};
use anyhow::{Context as _, Result};
use clap::Args;
use std::collections::HashMap;
use std::fs;

/// Command-line arguments for the `config` subcommand.
#[derive(Args)]
pub struct ConfigArgs {
    /// Set the COMPOSE_DATA directory path.
    #[arg(long, help = "Set COMPOSE_DATA directory path")]
    pub compose_data: Option<String>,

    /// Set the COMPOSE_BASE directory path.
    #[arg(long, help = "Set COMPOSE_BASE directory path")]
    pub compose_base: Option<String>,

    /// Set the ACME domain for Traefik.
    #[arg(long, help = "Set ACME domain for Traefik")]
    pub acme_domain: Option<String>,

    /// Set the ACME email for Traefik.
    #[arg(long, help = "Set ACME email for Traefik")]
    pub acme_email: Option<String>,

    /// Set the ACME server URL for Traefik.
    #[arg(long, help = "Set ACME server URL for Traefik")]
    pub acme_server: Option<String>,

    /// Set the DOCKER_HOST URI.
    #[arg(long, help = "Set DOCKER_HOST")]
    pub docker_host: Option<String>,
}

/// Entry point for the `config` command.
///
/// If any update arguments are provided, it performs an update.
/// Otherwise, it displays the current configuration.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `args` - The parsed command arguments.
pub fn run(ctx: &Context, args: ConfigArgs) -> Result<()> {
    if args.has_updates() {
        update_config(ctx, args)
    } else {
        show_config(ctx)
    }
}

impl ConfigArgs {
    /// Checks if any configuration update flags were provided.
    fn has_updates(&self) -> bool {
        self.compose_data.is_some()
            || self.compose_base.is_some()
            || self.acme_domain.is_some()
            || self.acme_email.is_some()
            || self.acme_server.is_some()
            || self.docker_host.is_some()
    }
}

/// Reads the `compose.env` file and returns it as a key-value map.
fn read_config(ctx: &Context) -> Result<HashMap<String, String>> {
    read_env_file(&ctx.env_file)
}

/// Writes the provided configuration map back to the `compose.env` file.
///
/// Preserves keys not specifically managed by this tool.
///
/// # Arguments
///
/// * `ctx` - The application context.
/// * `config` - The map of configuration keys and values.
fn write_config(ctx: &Context, config: &HashMap<String, String>) -> Result<()> {
    let mut lines = Vec::new();
    lines.push("# Compose Environment Configuration".to_string());
    lines.push(String::new());

    for key in CONFIG_KEYS {
        if let Some(value) = config.get(*key) {
            lines.push(format!("{}={}", key, value));
        }
    }

    for (key, value) in config {
        if !CONFIG_KEYS.contains(&key.as_str()) {
            lines.push(format!("{}={}", key, value));
        }
    }

    fs::write(&ctx.env_file, lines.join("\n") + "\n")
        .with_context(|| format!("Failed to write {}", ctx.env_file.display()))?;

    Ok(())
}

/// Formats and prints the current configuration to stdout.
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

    for (key, value) in &config {
        if !CONFIG_KEYS.contains(&key.as_str()) {
            println!("{:width$}  {}", key, value, width = max_key_len);
        }
    }

    Ok(())
}

/// Validates and updates the configuration based on provided arguments.
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

        fn env_file(&self) -> PathBuf {
            self.base.join("compose.env")
        }

        fn context(&self) -> Context {
            Context {
                is_root: false,
                systemd_dir: PathBuf::from("/tmp/test-systemd"),
                compose_base: self.base.clone(),
                env_file: self.env_file(),
                docker_host: None,
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

    #[test]
    fn test_read_config_nonexistent_file() {
        let test_dir = TestDir::new("read-nonexistent");
        let ctx = test_dir.context();
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
