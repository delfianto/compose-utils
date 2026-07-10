//! JSON output mode support.
//!
//! Mirrors the verbose flag pattern: a global switch that commands check
//! to decide between human-readable and machine-readable (JSON) output.
//! This lets agentic/scripted callers pass `--json` and get parseable
//! results instead of prose.

use serde::Serialize;
use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag indicating whether JSON output mode is enabled.
static JSON_MODE: AtomicBool = AtomicBool::new(false);

/// Enables JSON output mode.
pub fn enable() {
    JSON_MODE.store(true, Ordering::SeqCst);
}

/// Returns whether JSON output mode is currently enabled.
pub fn is_enabled() -> bool {
    JSON_MODE.load(Ordering::SeqCst)
}

/// Uniform envelope for command results: the command name plus a list of
/// per-item results.
#[derive(Serialize)]
pub struct Report<T: Serialize> {
    pub command: &'static str,
    pub results: Vec<T>,
}

/// Serializes `value` as pretty-printed JSON to stdout.
pub fn print_json<T: Serialize>(value: &T) -> anyhow::Result<()> {
    println!("{}", serde_json::to_string_pretty(value)?);
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_enable_sets_flag() {
        enable();
        assert!(is_enabled());
    }

    #[test]
    fn test_report_serializes() {
        #[derive(Serialize)]
        struct Item {
            name: String,
        }

        let report = Report {
            command: "test",
            results: vec![Item { name: "a".to_string() }],
        };

        let json = serde_json::to_string(&report).unwrap();
        assert!(json.contains("\"command\":\"test\""));
        assert!(json.contains("\"name\":\"a\""));
    }
}
