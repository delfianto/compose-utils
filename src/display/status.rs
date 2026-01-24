//! Utilities for formatting and interpreting Docker container status and states.

/// Parses a Docker status string to extract uptime and health indicators.
///
/// This function attempts to strip out health information and isolate the
/// human-readable uptime duration.
///
/// # Examples
///
/// - `"Up 49 seconds"` -> `("49 seconds", "🤍")`
/// - `"Up 49 seconds (healthy)"` -> `("49 seconds", "💚")`
/// - `"Exited (0) 2 hours ago"` -> `("-", "🤍")`
///
/// # Arguments
///
/// * `status` - The raw status string from the Docker API.
///
/// Returns a tuple of `(uptime_string, health_emoji)`.
pub fn parse_status_uptime_health(status: &str) -> (String, String) {
    let health = if status.contains("(healthy)") {
        "💚".to_string()
    } else if status.contains("(unhealthy)") {
        "💔".to_string()
    } else if status.contains("(health:") {
        "💛".to_string()
    } else {
        "🤍".to_string()
    };

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

/// Maps a raw Docker container state string to a representative emoji.
///
/// # Arguments
///
/// * `state` - The container state (e.g., "running", "paused", "exited").
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

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_parse_status_uptime_health() {
        assert_eq!(
            parse_status_uptime_health("Up 49 seconds"),
            ("49 seconds".to_string(), "🤍".to_string())
        );
        assert_eq!(
            parse_status_uptime_health("Up 5 minutes (healthy)"),
            ("5 minutes".to_string(), "💚".to_string())
        );
        assert_eq!(
            parse_status_uptime_health("Up 2 hours (unhealthy)"),
            ("2 hours".to_string(), "💔".to_string())
        );
        assert_eq!(
            parse_status_uptime_health("Up 10 seconds (health: starting)"),
            ("10 seconds".to_string(), "💛".to_string())
        );
        assert_eq!(
            parse_status_uptime_health("Exited (0) 3 hours ago"),
            ("-".to_string(), "🤍".to_string())
        );
    }

    #[test]
    fn test_state_to_emoji() {
        assert_eq!(state_to_emoji("running"), "🟢");
        assert_eq!(state_to_emoji("RUNNING"), "🟢");
        assert_eq!(state_to_emoji("exited"), "🛑");
        assert_eq!(state_to_emoji("something_else"), "❓");
    }
}
