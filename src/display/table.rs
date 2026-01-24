//! Logic for rendering Docker container information in a tabular format.

use crate::display::status::{parse_status_uptime_health, state_to_emoji};
use crate::docker::types::{ContainerInfo, PortInfo};
use chrono::{DateTime, Utc};

/// Renders a list of Docker containers into a formatted table printed to stdout.
///
/// The table includes columns for ID, name, image, creation date, uptime,
/// port mappings, state, and health.
///
/// # Arguments
///
/// * `containers` - A vector of [`ContainerInfo`] objects to display.
pub fn render_containers_table(containers: Vec<ContainerInfo>) {
    if containers.is_empty() {
        println!("No containers found.");
        return;
    }

    let headers = [
        "ID",
        "NAME",
        "IMAGE/TAG",
        "CREATED",
        "UPTIME",
        "PORTS",
        "STATE",
        "HEALTH",
    ];

    /// Internal structure representing a processed row of data.
    struct RowData {
        id: String,
        name: String,
        image: String,
        created: String,
        uptime: String,
        ports: Vec<String>,
        state: String,
        health: String,
    }
    let mut data: Vec<RowData> = Vec::new();

    let mut w_id = headers[0].len();
    let mut w_name = headers[1].len();
    let mut w_image = headers[2].len();
    let mut w_created = headers[3].len();
    let mut w_uptime = headers[4].len();
    let mut w_ports = headers[5].len();

    for c in containers {
        let id = &c.id;
        let id_short = if id.len() > 12 { &id[..12] } else { id };

        let name = c
            .names
            .first()
            .map(|s| s.strip_prefix('/').unwrap_or(s))
            .unwrap_or("<unknown>");

        let image = c.image.as_deref().unwrap_or("<unknown>");

        let created = c
            .created
            .map(|ts: i64| {
                DateTime::from_timestamp(ts, 0)
                    .map(|dt: DateTime<Utc>| dt.format("%Y-%m-%d %H:%M:%S").to_string())
                    .unwrap_or_else(|| ts.to_string())
            })
            .unwrap_or_else(|| "-".to_string());

        let status = c.status.as_deref().unwrap_or("");
        let (uptime, health) = parse_status_uptime_health(status);

        let state_str = c.state.as_deref().unwrap_or("unknown").to_lowercase();
        let state = state_to_emoji(&state_str).to_string();

        let ports: Vec<String> = c.ports.iter().map(format_port).collect();

        w_id = w_id.max(id_short.len());
        w_name = w_name.max(name.len());
        w_image = w_image.max(image.len());
        w_created = w_created.max(created.len());
        w_uptime = w_uptime.max(uptime.len());
        if let Some(first_port) = ports.first() {
            w_ports = w_ports.max(first_port.len());
        }

        data.push(RowData {
            id: id_short.to_string(),
            name: name.to_string(),
            image: image.to_string(),
            created,
            uptime,
            ports,
            state,
            health,
        });
    }

    print!(
        "{:<w_id$}  {:<w_name$}  {:<w_image$}  {:<w_created$}  {:<w_uptime$}  {:<w_ports$}  STATE  HEALTH",
        headers[0], headers[1], headers[2], headers[3], headers[4], headers[5]
    );
    println!();

    let port_indent = w_id + 2 + w_name + 2 + w_image + 2 + w_created + 2 + w_uptime + 2;

    for row in data {
        let first_port = row.ports.first().map(|s| s.as_str()).unwrap_or("");

        print!(
            "{:<w_id$}  {:<w_name$}  {:<w_image$}  {:<w_created$}  {:<w_uptime$}  {:<w_ports$}  {}     {}",
            row.id, row.name, row.image, row.created, row.uptime, first_port, row.state, row.health
        );
        println!();

        for port in row.ports.iter().skip(1) {
            println!("{:port_indent$}{}", "", port);
        }
    }
}

/// Formats a [`PortInfo`] object into a human-readable string.
///
/// # Examples
/// - `PortInfo { ip: Some("0.0.0.0"), private_port: 80, public_port: Some(8080), type_: Some("tcp") }`
///   -> `"0.0.0.0:8080->80/tcp"`
fn format_port(p: &PortInfo) -> String {
    let private = p.private_port;
    let port_type = p.type_.as_deref().unwrap_or("tcp");
    match (&p.ip, p.public_port) {
        (Some(ip), Some(public)) => {
            format!("{}:{}->{}/{}", ip, public, private, port_type)
        }
        (None, Some(public)) => format!("{}->{}/{}", public, private, port_type),
        _ => format!("{}/{}", private, port_type),
    }
}
