use colored::*;

/// Returns an emoji representation of the container state.
pub fn state_emoji(state: &str) -> &'static str {
    match state.to_lowercase().as_str() {
        "running" => "▶️",
        "created" => "🆕",
        "restarting" => "🔄",
        "exited" => "⏹️",
        "paused" => "⏸️",
        "dead" => "💀",
        "removing" => "🔥",
        _ => "❓",
    }
}

/// Returns a colored dot representation of the health status.
pub fn health_dot(status: &str) -> String {
    if status.contains("(healthy)") {
        "●".green().to_string()
    } else if status.contains("(unhealthy)") {
        "●".red().to_string()
    } else if status.contains("(starting)") {
        "●".yellow().to_string()
    } else {
        "".to_string()
    }
}

/// Formats the full status string with emoji and health dot.

pub fn format_status(state: &str, status: &str) -> String {
    let emoji = state_emoji(state);

    let dot = health_dot(status);

    let clean_status = if !dot.is_empty() {
        status
            .replace(" (healthy)", "")
            .replace(" (unhealthy)", "")
            .replace(" (starting)", "")
    } else {
        status.to_string()
    };

    if dot.is_empty() {
        format!("{} {}", emoji, clean_status)
    } else {
        format!("{} {} {}", emoji, clean_status, dot)
    }
}
