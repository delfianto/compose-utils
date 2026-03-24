//! Logic for the `ps` command.

use crate::core::Context;
use crate::display::status::format_status;
use crate::display::table::Table;
use anyhow::{Context as _, Result};
use serde::Deserialize;
use std::process::Command;

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct DockerContainer {
    #[serde(rename = "ID")]
    id: String,
    image: String,
    names: String,
    ports: String,
    state: String,
    status: String,
}

/// Executes the `ps` command to list Docker containers.
pub fn run_ps(_ctx: &Context, _services: &[String]) -> Result<()> {
    let output = Command::new("docker")
        .arg("ps")
        .arg("-a")
        .arg("--format")
        .arg("{{json .}}")
        .output()
        .context("Failed to execute docker ps")?;

    if !output.status.success() {
        let err = String::from_utf8_lossy(&output.stderr);
        return Err(anyhow::anyhow!("docker ps failed: {}", err));
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut containers = Vec::new();

    for line in stdout.lines() {
        if line.trim().is_empty() {
            continue;
        }
        let container: DockerContainer =
            serde_json::from_str(line).context("Failed to parse docker ps output")?;
        containers.push(container);
    }

    if containers.is_empty() {
        println!("No containers found.");
        return Ok(());
    }

    let mut table = Table::new(vec!["ID", "IMAGE/TAG", "NAME", "PORTS", "STATUS"]);

    for c in containers {
        let formatted_ports = c.ports.replace(", ", "\n");
        let formatted_status = format_status(&c.state, &c.status);

        table.add_row(vec![
            c.id,
            c.image,
            c.names,
            formatted_ports,
            formatted_status,
        ]);
    }

    print!("{}", table.render());

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_deserialize_docker_container() {
        let json = r#"{
            "ID": "abc123",
            "Image": "nginx:latest",
            "Names": "web",
            "Ports": "0.0.0.0:80->80/tcp",
            "State": "running",
            "Status": "Up 5 hours"
        }"#;

        let container: DockerContainer = serde_json::from_str(json).unwrap();
        assert_eq!(container.id, "abc123");
        assert_eq!(container.image, "nginx:latest");
        assert_eq!(container.names, "web");
        assert_eq!(container.ports, "0.0.0.0:80->80/tcp");
        assert_eq!(container.state, "running");
        assert_eq!(container.status, "Up 5 hours");
    }

    #[test]
    fn test_deserialize_empty_ports() {
        let json = r#"{
            "ID": "abc123",
            "Image": "redis:7",
            "Names": "cache",
            "Ports": "",
            "State": "running",
            "Status": "Up 1 hour (healthy)"
        }"#;

        let container: DockerContainer = serde_json::from_str(json).unwrap();
        assert_eq!(container.ports, "");
        assert_eq!(container.status, "Up 1 hour (healthy)");
    }

    #[test]
    fn test_deserialize_exited_container() {
        let json = r#"{
            "ID": "def456",
            "Image": "myapp:v2",
            "Names": "worker",
            "Ports": "",
            "State": "exited",
            "Status": "Exited (137) 2 hours ago"
        }"#;

        let container: DockerContainer = serde_json::from_str(json).unwrap();
        assert_eq!(container.state, "exited");
        assert!(container.status.contains("137"));
    }

    #[test]
    fn test_deserialize_missing_field_fails() {
        let json = r#"{
            "ID": "abc123",
            "Image": "nginx"
        }"#;

        let result: Result<DockerContainer, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    #[test]
    fn test_deserialize_multiple_ports() {
        let json = r#"{
            "ID": "abc123",
            "Image": "nginx",
            "Names": "web",
            "Ports": "0.0.0.0:80->80/tcp, 0.0.0.0:443->443/tcp",
            "State": "running",
            "Status": "Up 5 hours"
        }"#;

        let container: DockerContainer = serde_json::from_str(json).unwrap();
        assert!(container.ports.contains("80"));
        assert!(container.ports.contains("443"));
    }

    #[test]
    fn test_port_formatting_logic() {
        let ports = "0.0.0.0:80->80/tcp, 0.0.0.0:443->443/tcp";
        let formatted = ports.replace(", ", "\n");
        assert_eq!(formatted, "0.0.0.0:80->80/tcp\n0.0.0.0:443->443/tcp");
    }

    #[test]
    fn test_port_formatting_no_separator() {
        let ports = "0.0.0.0:80->80/tcp";
        let formatted = ports.replace(", ", "\n");
        assert_eq!(formatted, "0.0.0.0:80->80/tcp");
    }

    #[test]
    fn test_port_formatting_empty() {
        let ports = "";
        let formatted = ports.replace(", ", "\n");
        assert_eq!(formatted, "");
    }
}
