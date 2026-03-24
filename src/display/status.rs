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

#[cfg(test)]
mod tests {
    use super::*;

    // --- state_emoji ---

    #[test]
    fn test_state_emoji_running() {
        assert_eq!(state_emoji("running"), "▶️");
    }

    #[test]
    fn test_state_emoji_created() {
        assert_eq!(state_emoji("created"), "🆕");
    }

    #[test]
    fn test_state_emoji_restarting() {
        assert_eq!(state_emoji("restarting"), "🔄");
    }

    #[test]
    fn test_state_emoji_exited() {
        assert_eq!(state_emoji("exited"), "⏹️");
    }

    #[test]
    fn test_state_emoji_paused() {
        assert_eq!(state_emoji("paused"), "⏸️");
    }

    #[test]
    fn test_state_emoji_dead() {
        assert_eq!(state_emoji("dead"), "💀");
    }

    #[test]
    fn test_state_emoji_removing() {
        assert_eq!(state_emoji("removing"), "🔥");
    }

    #[test]
    fn test_state_emoji_unknown() {
        assert_eq!(state_emoji("something_else"), "❓");
    }

    #[test]
    fn test_state_emoji_empty() {
        assert_eq!(state_emoji(""), "❓");
    }

    #[test]
    fn test_state_emoji_case_insensitive() {
        assert_eq!(state_emoji("Running"), "▶️");
        assert_eq!(state_emoji("RUNNING"), "▶️");
        assert_eq!(state_emoji("Exited"), "⏹️");
        assert_eq!(state_emoji("DEAD"), "💀");
    }

    // --- health_dot ---

    #[test]
    fn test_health_dot_healthy() {
        let dot = health_dot("Up 5 hours (healthy)");
        assert!(dot.contains("●"));
        assert!(!dot.is_empty());
    }

    #[test]
    fn test_health_dot_unhealthy() {
        let dot = health_dot("Up 5 hours (unhealthy)");
        assert!(dot.contains("●"));
        assert!(!dot.is_empty());
    }

    #[test]
    fn test_health_dot_starting() {
        let dot = health_dot("Up 5 seconds (starting)");
        assert!(dot.contains("●"));
        assert!(!dot.is_empty());
    }

    #[test]
    fn test_health_dot_no_health_info() {
        assert_eq!(health_dot("Up 5 hours"), "");
    }

    #[test]
    fn test_health_dot_empty_status() {
        assert_eq!(health_dot(""), "");
    }

    #[test]
    fn test_health_dot_partial_match() {
        // "healthy" without parentheses should not match
        assert_eq!(health_dot("healthy"), "");
    }

    // --- format_status ---

    #[test]
    fn test_format_status_running_no_health() {
        let result = format_status("running", "Up 5 hours");
        assert!(result.contains("▶️"));
        assert!(result.contains("Up 5 hours"));
    }

    #[test]
    fn test_format_status_running_healthy() {
        let result = format_status("running", "Up 5 hours (healthy)");
        assert!(result.contains("▶️"));
        assert!(result.contains("Up 5 hours"));
        // Health indicator stripped from text, dot appended
        assert!(!result.contains("(healthy)"));
        assert!(result.contains("●"));
    }

    #[test]
    fn test_format_status_exited_no_health() {
        let result = format_status("exited", "Exited (0) 3 hours ago");
        assert!(result.contains("⏹️"));
        assert!(result.contains("Exited (0) 3 hours ago"));
    }

    #[test]
    fn test_format_status_unknown_state() {
        let result = format_status("weird", "Some status");
        assert!(result.contains("❓"));
        assert!(result.contains("Some status"));
    }

    #[test]
    fn test_format_status_empty_state_and_status() {
        let result = format_status("", "");
        assert!(result.contains("❓"));
    }

    #[test]
    fn test_format_status_unhealthy() {
        let result = format_status("running", "Up 1 hour (unhealthy)");
        assert!(!result.contains("(unhealthy)"));
        assert!(result.contains("●"));
    }

    #[test]
    fn test_format_status_starting_health() {
        let result = format_status("running", "Up 3 seconds (starting)");
        assert!(!result.contains("(starting)"));
        assert!(result.contains("●"));
    }
}
