//! Input validation utilities for configuration values.

use anyhow::{Context as _, Result, bail};
use regex::Regex;
use std::net::ToSocketAddrs;
use std::path::Path;
use std::sync::LazyLock;
use url::Url;

/// Regex for validating domain names (RFC 1035 compliant).
static DOMAIN_RE: LazyLock<Regex> = LazyLock::new(|| {
    Regex::new(r"^([a-zA-Z0-9]([a-zA-Z0-9\-]{0,61}[a-zA-Z0-9])?\.)+[a-zA-Z]{2,}$").unwrap()
});

/// Regex for validating email addresses (simplified RFC 5322).
static EMAIL_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^[a-zA-Z0-9._%+-]+@[a-zA-Z0-9.-]+\.[a-zA-Z]{2,}$").unwrap());

/// Ensures that a path refers to an existing directory.
pub fn validate_directory(path: &str, field_name: &str) -> Result<()> {
    let path = Path::new(path);
    if !path.exists() {
        bail!("{} path does not exist: {}", field_name, path.display());
    }
    if !path.is_dir() {
        bail!("{} path is not a directory: {}", field_name, path.display());
    }
    Ok(())
}

/// Validates that a string resembles a valid domain name.
pub fn validate_domain(domain: &str) -> Result<()> {
    if !DOMAIN_RE.is_match(domain) {
        bail!(
            "TRAEFIK_ACME_DOMAIN must be a valid domain name (e.g., example.com): {}",
            domain
        );
    }
    Ok(())
}

/// Validates that a string is a correctly formatted email address.
pub fn validate_email(email: &str) -> Result<()> {
    if !EMAIL_RE.is_match(email) {
        bail!(
            "TRAEFIK_ACME_EMAIL must be a valid email address: {}",
            email
        );
    }
    Ok(())
}

/// Validates that a URL is well-formed and its host is network-resolvable.
pub fn validate_acme_server(server: &str) -> Result<()> {
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

/// Validates a Docker host string (supporting `unix://`, `tcp://`, and `ssh://`).
pub fn validate_docker_host(host: &str) -> Result<()> {
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
    use std::fs;
    use std::path::PathBuf;

    struct TestDir {
        base: PathBuf,
    }

    impl TestDir {
        fn new(name: &str) -> Self {
            let base = PathBuf::from(format!(
                "/tmp/compose-validation-test-{}-{}",
                name,
                std::process::id()
            ));
            fs::create_dir_all(&base).unwrap();
            Self { base }
        }

        fn path(&self) -> &Path {
            &self.base
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
        assert!(validate_acme_server("https://acme-v02.api.letsencrypt.org/directory").is_ok());
    }

    #[test]
    fn test_validate_acme_server_http() {
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
}
