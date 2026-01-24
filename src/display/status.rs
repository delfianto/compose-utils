/// Parse docker status to extract uptime and health
/// Examples: "Up 49 seconds" -> ("49 seconds", "-")
///           "Up 49 seconds (healthy)" -> ("49 seconds", "✓")
///           "Up About an hour" -> ("About an hour", "-")
///           "Exited (0) 2 hours ago" -> ("-", "-")
pub fn parse_status_uptime_health(status: &str) -> (String, String) {
    // Extract health if present (e.g., "(healthy)", "(unhealthy)")
    let health = if status.contains("(healthy)") {
        "💚".to_string()
    } else if status.contains("(unhealthy)") {
        "💔".to_string()
    } else if status.contains("(health:") {
        // Handle "(health: starting)" etc.
        "💛".to_string()
    } else {
        "🤍".to_string()
    };

    // Extract uptime - remove health status in parentheses first
    let status_without_health = status.split('(').next().unwrap_or(status).trim();

    let uptime = if status_without_health.starts_with("Up ") {
        status_without_health
            .strip_prefix("Up ")
            .unwrap_or("-")
            .to_string()
    } else {
        "-".to_string()
    };

    (uptime, health)
}

/// Map docker container state to emoji
pub fn state_to_emoji(state: &str) -> &'static str {
    match state.to_lowercase().as_str() {
        "created" => "🏗️",
        "running" => "🟢",
        "restarting" => "🔄",
        "paused" => "⏸️",
        "exited" => "🛑",
        "removing" => "🚮",
        "dead" => "💀",
        _ => "❓",
    }
}
