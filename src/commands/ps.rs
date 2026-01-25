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
pub async fn run_ps(_ctx: &Context, _services: &[String]) -> Result<()> {
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
