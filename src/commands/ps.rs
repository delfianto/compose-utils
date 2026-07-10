//! Logic for the `ps` command.
//!
//! Mirrors the output of the `docker pps` CLI plugin
//! (`~/.config/docker/cli-plugins/docker-pps`, brief format): a compact
//! NAMES/ID/IMAGE/STATUS/PORTS table with a colored bullet + elapsed-time
//! status cell, instead of docker's verbose "Up 5 hours (healthy)" text.

use crate::core::{Context, Report};
use anyhow::{Context as _, Result};
use regex::Regex;
use serde::{Deserialize, Serialize};
use std::collections::{HashMap, HashSet};
use std::process::Command;
use std::sync::LazyLock;
use std::time::{SystemTime, UNIX_EPOCH};

const RED: &str = "\x1b[0;31m";
const GREEN: &str = "\x1b[0;32m";
const YELLOW: &str = "\x1b[1;33m";
const BLUE: &str = "\x1b[0;34m";
const CYAN: &str = "\x1b[1;36m";
const NC: &str = "\x1b[0m";

/// Matches a bare container ID/digest, indicating `docker ps`'s Image field
/// is a dangling reference (the tag has since moved to a newer image).
static BARE_ID_RE: LazyLock<Regex> =
    LazyLock::new(|| Regex::new(r"^([0-9a-f]{12,64}|sha256:[0-9a-f]+)$").unwrap());

#[derive(Debug, Deserialize)]
#[serde(rename_all = "PascalCase")]
struct PsEntry {
    #[serde(rename = "ID")]
    id: String,
    names: String,
    image: String,
    ports: String,
    state: String,
    status: String,
}

/// Parsed `docker inspect` detail for one container.
struct InspectDetail {
    image: String,
    state: String,
    started: Option<i64>,
    finished: Option<i64>,
    health: String,
}

/// A single row ready for rendering or JSON serialization.
struct Row {
    name: String,
    id: String,
    image: String,
    ports: Vec<String>,
    state: String,
    health: Option<String>,
    started: Option<i64>,
    finished: Option<i64>,
    /// False when `docker inspect` had no data for this container (rare
    /// race between `docker ps` and `docker inspect`); falls back to
    /// docker ps's own (uncolored) Status text.
    inspected: bool,
    raw_status: String,
}

/// JSON-friendly container result.
#[derive(Serialize)]
struct ContainerResult {
    name: String,
    id: String,
    image: String,
    state: String,
    health: Option<String>,
    uptime: Option<String>,
    ports: Vec<String>,
}

/// Executes the `ps` command to list Docker containers.
pub fn run_ps(ctx: &Context, _services: &[String]) -> Result<()> {
    let mut entries = list_containers(ctx)?;

    if entries.is_empty() {
        if crate::core::is_json() {
            crate::core::print_json(&Report::<ContainerResult> {
                command: "ps",
                results: Vec::new(),
            })?;
        } else {
            println!("No containers found");
        }
        return Ok(());
    }

    entries.sort_by(|a, b| a.names.cmp(&b.names));

    let ids: Vec<String> = entries.iter().map(|e| e.id.clone()).collect();
    let details = inspect_details(ctx, &ids)?;
    let rows: Vec<Row> = entries.into_iter().map(|e| build_row(e, &details)).collect();

    if crate::core::is_json() {
        let results = rows
            .iter()
            .map(|r| ContainerResult {
                name: r.name.clone(),
                id: r.id.clone(),
                image: r.image.clone(),
                state: r.state.clone(),
                health: r.health.clone(),
                uptime: uptime_json(r),
                ports: r.ports.clone(),
            })
            .collect();
        crate::core::print_json(&Report { command: "ps", results })?;
        return Ok(());
    }

    render_table(&rows);
    Ok(())
}

fn list_containers(ctx: &Context) -> Result<Vec<PsEntry>> {
    let mut cmd = Command::new("docker");
    cmd.args(["ps", "--all", "--format", "{{json .}}"]);
    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    let output = cmd.output().context("Failed to execute docker ps")?;

    if !output.status.success() {
        anyhow::bail!("docker ps failed: {}", String::from_utf8_lossy(&output.stderr));
    }

    String::from_utf8_lossy(&output.stdout)
        .lines()
        .filter(|l| !l.trim().is_empty())
        .map(|line| serde_json::from_str(line).context("Failed to parse docker ps output"))
        .collect()
}

/// Batch-inspects containers for their original image name, state, health,
/// and start/finish timestamps. Returns an empty map (triggering the raw
/// docker ps fallback for every row) if `docker inspect` itself fails.
fn inspect_details(ctx: &Context, ids: &[String]) -> Result<HashMap<String, InspectDetail>> {
    let fmt = "{{.Config.Image}}\t{{.State.Status}}\t{{.State.StartedAt}}\t\
               {{.State.FinishedAt}}\t{{with .State.Health}}{{.Status}}{{end}}";

    let mut cmd = Command::new("docker");
    cmd.arg("inspect").arg("--format").arg(fmt).args(ids);
    if let Some(ref host) = ctx.docker_host {
        cmd.env("DOCKER_HOST", host);
    }

    let output = cmd.output().context("Failed to execute docker inspect")?;
    if !output.status.success() {
        return Ok(HashMap::new());
    }

    let stdout = String::from_utf8_lossy(&output.stdout);
    let mut details = HashMap::new();

    for (id, line) in ids.iter().zip(stdout.lines()) {
        let parts: Vec<&str> = line.split('\t').collect();
        if parts.len() != 5 {
            continue;
        }
        details.insert(
            id.clone(),
            InspectDetail {
                image: parts[0].to_string(),
                state: parts[1].to_string(),
                started: parse_docker_time(parts[2]),
                finished: parse_docker_time(parts[3]),
                health: parts[4].to_string(),
            },
        );
    }

    Ok(details)
}

fn build_row(entry: PsEntry, details: &HashMap<String, InspectDetail>) -> Row {
    let ports = parse_port_lines(&entry.ports);

    match details.get(&entry.id) {
        Some(detail) => {
            // docker ps shows a bare ID when the tag has since moved to a
            // newer image; recover the name the container was started from.
            let image = if BARE_ID_RE.is_match(&entry.image) && !detail.image.is_empty() {
                format!("{} (outdated)", detail.image)
            } else {
                entry.image
            };

            Row {
                name: entry.names,
                id: entry.id,
                image,
                ports,
                state: detail.state.clone(),
                health: if detail.health.is_empty() { None } else { Some(detail.health.clone()) },
                started: detail.started,
                finished: detail.finished,
                inspected: true,
                raw_status: entry.status,
            }
        }
        None => Row {
            name: entry.names,
            id: entry.id,
            image: entry.image,
            ports,
            state: entry.state,
            health: None,
            started: None,
            finished: None,
            inspected: false,
            raw_status: entry.status,
        },
    }
}

/// Builds the STATUS column cell: glyph + elapsed clock, plus a color.
///
/// Running shows uptime, stopped shows time since exit; color carries
/// health (green ok, yellow starting/paused, red unhealthy/exited).
fn status_cell(row: &Row) -> (String, &'static str) {
    if !row.inspected {
        return (row.raw_status.clone(), "");
    }

    match row.state.as_str() {
        "running" | "restarting" | "paused" => {
            let color = if row.state == "running" {
                match row.health.as_deref() {
                    Some("starting") => YELLOW,
                    Some("unhealthy") => RED,
                    _ => GREEN,
                }
            } else {
                YELLOW
            };
            (format!("\u{25cf} {}", format_clock(row.started)), color)
        }
        "created" => ("\u{25cb} --:--".to_string(), BLUE),
        // exited, dead, removing
        _ => (format!("\u{25cb} {}", format_clock(row.finished)), RED),
    }
}

/// JSON-mode uptime: `None` when there's nothing meaningful to report (a
/// freshly created container that hasn't started, or missing inspect data).
fn uptime_json(row: &Row) -> Option<String> {
    if !row.inspected {
        return None;
    }
    match row.state.as_str() {
        "running" | "restarting" | "paused" => row.started.map(|s| format_clock(Some(s))),
        "created" => None,
        _ => row.finished.map(|f| format_clock(Some(f))),
    }
}

/// A row's rendered STATUS text (plain, pre-padding) plus its color, paired
/// with the rest of the row's already-borrowed display fields.
struct Cell<'a> {
    name: &'a str,
    id: &'a str,
    image: &'a str,
    status: String,
    color: &'static str,
    ports: &'a [String],
}

fn render_table(rows: &[Row]) {
    let cells: Vec<Cell> = rows
        .iter()
        .map(|r| {
            let (status, color) = status_cell(r);
            Cell { name: &r.name, id: &r.id, image: &r.image, status, color, ports: &r.ports }
        })
        .collect();

    let name_w = cells.iter().map(|c| c.name.chars().count()).max().unwrap_or(0).max(5);
    let id_w = cells.iter().map(|c| c.id.chars().count()).max().unwrap_or(0).max(2);
    let image_w = cells.iter().map(|c| c.image.chars().count()).max().unwrap_or(0).max(5);
    let status_w = cells.iter().map(|c| c.status.chars().count()).max().unwrap_or(0).max(6);

    let header = format!(
        "{:<name_w$}  {:<id_w$}  {:<image_w$}  {:<status_w$}  PORTS",
        "NAMES",
        "ID",
        "IMAGE",
        "STATUS",
        name_w = name_w,
        id_w = id_w,
        image_w = image_w,
        status_w = status_w,
    );
    println!("{CYAN}{header}{NC}");

    let indent = " ".repeat(name_w + id_w + image_w + status_w + 8);

    for cell in &cells {
        let status_padded = format!("{:<status_w$}", cell.status, status_w = status_w);
        let status_col = if cell.color.is_empty() {
            status_padded
        } else {
            format!("{}{status_padded}{NC}", cell.color)
        };

        let first_port = cell.ports.first().map(String::as_str).unwrap_or("-");
        println!(
            "{:<name_w$}  {:<id_w$}  {:<image_w$}  {status_col}  {first_port}",
            cell.name,
            cell.id,
            cell.image,
            name_w = name_w,
            id_w = id_w,
            image_w = image_w,
        );

        for extra in cell.ports.iter().skip(1) {
            println!("{indent}{extra}");
        }
    }
}

/// Splits docker's ports string into one readable entry per line: bound
/// ports (`host->container`) sorted by host port, then a single line
/// combining any purely-exposed (unbound) ports. Drops IPv6 wildcard binds
/// that duplicate an IPv4 wildcard bind on the same port.
fn parse_port_lines(ports: &str) -> Vec<String> {
    if ports.is_empty() {
        return vec!["-".to_string()];
    }

    let entries: Vec<&str> = ports.split(',').map(|e| e.trim()).filter(|e| !e.is_empty()).collect();
    let (mut bound, exposed): (Vec<&str>, Vec<&str>) = entries.into_iter().partition(|e| e.contains("->"));

    let bound_set: HashSet<&str> = bound.iter().copied().collect();
    bound.retain(|e| match e.strip_prefix("[::]:") {
        Some(rest) => !bound_set.contains(format!("0.0.0.0:{}", rest).as_str()),
        None => true,
    });

    bound.sort_by_key(|e| host_port(e));

    let mut lines: Vec<String> = bound.into_iter().map(|s| s.to_string()).collect();
    if !exposed.is_empty() {
        lines.push(exposed.join(", "));
    }

    if lines.is_empty() { vec!["-".to_string()] } else { lines }
}

fn host_port(entry: &str) -> u32 {
    let host = entry.split("->").next().unwrap_or("");
    let port = host.rsplit(':').next().unwrap_or("");
    port.split('-').next().unwrap_or("").parse().unwrap_or(0)
}

/// Parses a docker RFC3339 timestamp (always UTC, `Z`-suffixed) into Unix
/// epoch seconds; `None` for empty/zero-value timestamps (e.g.
/// `FinishedAt` on a still-running container).
fn parse_docker_time(ts: &str) -> Option<i64> {
    if ts.is_empty() || ts.starts_with("0001-01-01") {
        return None;
    }
    let ts = ts.strip_suffix('Z')?;
    let (date, time) = ts.split_once('T')?;

    let mut d = date.splitn(3, '-');
    let year: i64 = d.next()?.parse().ok()?;
    let month: u32 = d.next()?.parse().ok()?;
    let day: u32 = d.next()?.parse().ok()?;

    let time = time.split('.').next().unwrap_or(time);
    let mut t = time.splitn(3, ':');
    let hour: i64 = t.next()?.parse().ok()?;
    let minute: i64 = t.next()?.parse().ok()?;
    let second: i64 = t.next()?.parse().ok()?;

    Some(days_from_civil(year, month, day) * 86_400 + hour * 3600 + minute * 60 + second)
}

/// Days since the Unix epoch (1970-01-01) for a proleptic Gregorian date.
/// Standard "days_from_civil" algorithm (Howard Hinnant).
fn days_from_civil(y: i64, m: u32, d: u32) -> i64 {
    let y = if m <= 2 { y - 1 } else { y };
    let era = if y >= 0 { y } else { y - 399 } / 400;
    let yoe = y - era * 400;
    let mp = (i64::from(m) + 9) % 12;
    let doy = (153 * mp + 2) / 5 + i64::from(d) - 1;
    let doe = yoe * 365 + yoe / 4 - yoe / 100 + doy;
    era * 146_097 + doe - 719_468
}

/// Formats elapsed time since a Unix timestamp as `HH:MM`, or `Xd HH:MM`
/// past a day; `--:--` when there's no timestamp to measure from.
fn format_clock(epoch: Option<i64>) -> String {
    let Some(then) = epoch else {
        return "--:--".to_string();
    };

    let now = SystemTime::now()
        .duration_since(UNIX_EPOCH)
        .map(|d| d.as_secs() as i64)
        .unwrap_or(then);
    let elapsed = (now - then).max(0);

    let minutes_total = elapsed / 60;
    let days = minutes_total / (24 * 60);
    let hours = (minutes_total / 60) % 24;
    let minutes = minutes_total % 60;

    if days > 0 {
        format!("{}d {:02}:{:02}", days, hours, minutes)
    } else {
        format!("{:02}:{:02}", hours, minutes)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    // --- PsEntry deserialization ---

    #[test]
    fn test_deserialize_ps_entry() {
        let json = r#"{
            "ID": "abc123",
            "Image": "nginx:latest",
            "Names": "web",
            "Ports": "0.0.0.0:80->80/tcp",
            "State": "running",
            "Status": "Up 5 hours"
        }"#;
        let entry: PsEntry = serde_json::from_str(json).unwrap();
        assert_eq!(entry.id, "abc123");
        assert_eq!(entry.image, "nginx:latest");
        assert_eq!(entry.names, "web");
    }

    #[test]
    fn test_deserialize_ps_entry_missing_field_fails() {
        let json = r#"{"ID": "abc123", "Image": "nginx"}"#;
        let result: Result<PsEntry, _> = serde_json::from_str(json);
        assert!(result.is_err());
    }

    // --- BARE_ID_RE ---

    #[test]
    fn test_bare_id_re_matches_hex_id() {
        assert!(BARE_ID_RE.is_match("a1b2c3d4e5f6"));
        assert!(BARE_ID_RE.is_match(&"a".repeat(64)));
    }

    #[test]
    fn test_bare_id_re_matches_sha256() {
        assert!(BARE_ID_RE.is_match("sha256:a1b2c3d4e5f6"));
    }

    #[test]
    fn test_bare_id_re_rejects_tagged_image() {
        assert!(!BARE_ID_RE.is_match("nginx:latest"));
        assert!(!BARE_ID_RE.is_match("ghcr.io/foo/bar:v1"));
    }

    #[test]
    fn test_bare_id_re_rejects_too_short() {
        assert!(!BARE_ID_RE.is_match("a1b2c3"));
    }

    // --- days_from_civil / parse_docker_time ---

    #[test]
    fn test_days_from_civil_epoch() {
        assert_eq!(days_from_civil(1970, 1, 1), 0);
    }

    #[test]
    fn test_days_from_civil_known_date() {
        // 2024-01-01 is 19723 days after the epoch.
        assert_eq!(days_from_civil(2024, 1, 1), 19723);
    }

    #[test]
    fn test_parse_docker_time_valid() {
        let epoch = parse_docker_time("2024-01-01T00:00:00.123456789Z").unwrap();
        assert_eq!(epoch, 19723 * 86_400);
    }

    #[test]
    fn test_parse_docker_time_no_fraction() {
        let epoch = parse_docker_time("2024-01-01T00:00:00Z").unwrap();
        assert_eq!(epoch, 19723 * 86_400);
    }

    #[test]
    fn test_parse_docker_time_zero_value() {
        assert_eq!(parse_docker_time("0001-01-01T00:00:00Z"), None);
    }

    #[test]
    fn test_parse_docker_time_empty() {
        assert_eq!(parse_docker_time(""), None);
    }

    // --- format_clock ---

    #[test]
    fn test_format_clock_none() {
        assert_eq!(format_clock(None), "--:--");
    }

    #[test]
    fn test_format_clock_under_a_day() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let since = now - 3661; // 1h 01m ago
        assert_eq!(format_clock(Some(since)), "01:01");
    }

    #[test]
    fn test_format_clock_multi_day() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let since = now - (2 * 86_400 + 3 * 3600 + 4 * 60); // 2d 03:04 ago
        assert_eq!(format_clock(Some(since)), "2d 03:04");
    }

    #[test]
    fn test_format_clock_future_timestamp_clamped() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        assert_eq!(format_clock(Some(now + 1000)), "00:00");
    }

    // --- host_port ---

    #[test]
    fn test_host_port_simple() {
        assert_eq!(host_port("0.0.0.0:8080->80/tcp"), 8080);
    }

    #[test]
    fn test_host_port_range() {
        assert_eq!(host_port("0.0.0.0:8000-8010->8000-8010/tcp"), 8000);
    }

    #[test]
    fn test_host_port_no_match() {
        assert_eq!(host_port("garbage"), 0);
    }

    // --- parse_port_lines ---

    #[test]
    fn test_parse_port_lines_empty() {
        assert_eq!(parse_port_lines(""), vec!["-".to_string()]);
    }

    #[test]
    fn test_parse_port_lines_single_bound() {
        assert_eq!(
            parse_port_lines("0.0.0.0:80->80/tcp"),
            vec!["0.0.0.0:80->80/tcp".to_string()]
        );
    }

    #[test]
    fn test_parse_port_lines_sorted_by_host_port() {
        let result = parse_port_lines("0.0.0.0:443->443/tcp, 0.0.0.0:80->80/tcp");
        assert_eq!(
            result,
            vec!["0.0.0.0:80->80/tcp".to_string(), "0.0.0.0:443->443/tcp".to_string()]
        );
    }

    #[test]
    fn test_parse_port_lines_drops_ipv6_duplicate() {
        let result = parse_port_lines("0.0.0.0:80->80/tcp, [::]:80->80/tcp");
        assert_eq!(result, vec!["0.0.0.0:80->80/tcp".to_string()]);
    }

    #[test]
    fn test_parse_port_lines_keeps_ipv6_without_ipv4_match() {
        let result = parse_port_lines("[::]:80->80/tcp");
        assert_eq!(result, vec!["[::]:80->80/tcp".to_string()]);
    }

    #[test]
    fn test_parse_port_lines_exposed_only() {
        let result = parse_port_lines("3000/tcp");
        assert_eq!(result, vec!["3000/tcp".to_string()]);
    }

    #[test]
    fn test_parse_port_lines_bound_and_exposed() {
        let result = parse_port_lines("0.0.0.0:80->80/tcp, 3000/tcp");
        assert_eq!(result, vec!["0.0.0.0:80->80/tcp".to_string(), "3000/tcp".to_string()]);
    }

    // --- status_cell ---

    fn make_row(state: &str, health: Option<&str>, started: Option<i64>, finished: Option<i64>) -> Row {
        Row {
            name: "test".to_string(),
            id: "abc".to_string(),
            image: "img".to_string(),
            ports: vec!["-".to_string()],
            state: state.to_string(),
            health: health.map(str::to_string),
            started,
            finished,
            inspected: true,
            raw_status: String::new(),
        }
    }

    #[test]
    fn test_status_cell_running_healthy() {
        let row = make_row("running", Some("healthy"), Some(0), None);
        let (text, color) = status_cell(&row);
        assert!(text.starts_with('\u{25cf}'));
        assert_eq!(color, GREEN);
    }

    #[test]
    fn test_status_cell_running_no_health() {
        let row = make_row("running", None, Some(0), None);
        let (_, color) = status_cell(&row);
        assert_eq!(color, GREEN);
    }

    #[test]
    fn test_status_cell_running_starting() {
        let row = make_row("running", Some("starting"), Some(0), None);
        let (_, color) = status_cell(&row);
        assert_eq!(color, YELLOW);
    }

    #[test]
    fn test_status_cell_running_unhealthy() {
        let row = make_row("running", Some("unhealthy"), Some(0), None);
        let (_, color) = status_cell(&row);
        assert_eq!(color, RED);
    }

    #[test]
    fn test_status_cell_restarting() {
        let row = make_row("restarting", None, Some(0), None);
        let (_, color) = status_cell(&row);
        assert_eq!(color, YELLOW);
    }

    #[test]
    fn test_status_cell_created() {
        let row = make_row("created", None, None, None);
        let (text, color) = status_cell(&row);
        assert_eq!(text, "\u{25cb} --:--");
        assert_eq!(color, BLUE);
    }

    #[test]
    fn test_status_cell_exited() {
        let row = make_row("exited", None, None, Some(0));
        let (text, color) = status_cell(&row);
        assert!(text.starts_with('\u{25cb}'));
        assert_eq!(color, RED);
    }

    #[test]
    fn test_status_cell_not_inspected_uses_raw_status() {
        let mut row = make_row("running", None, Some(0), None);
        row.inspected = false;
        row.raw_status = "Up 5 hours".to_string();
        let (text, color) = status_cell(&row);
        assert_eq!(text, "Up 5 hours");
        assert_eq!(color, "");
    }

    // --- uptime_json ---

    #[test]
    fn test_uptime_json_running() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let row = make_row("running", None, Some(now - 60), None);
        assert_eq!(uptime_json(&row), Some("00:01".to_string()));
    }

    #[test]
    fn test_uptime_json_created_is_none() {
        let row = make_row("created", None, None, None);
        assert_eq!(uptime_json(&row), None);
    }

    #[test]
    fn test_uptime_json_not_inspected_is_none() {
        let mut row = make_row("running", None, Some(0), None);
        row.inspected = false;
        assert_eq!(uptime_json(&row), None);
    }

    #[test]
    fn test_uptime_json_exited_uses_finished() {
        let now = SystemTime::now().duration_since(UNIX_EPOCH).unwrap().as_secs() as i64;
        let row = make_row("exited", None, None, Some(now - 120));
        assert_eq!(uptime_json(&row), Some("00:02".to_string()));
    }
}
