//! Logic for reading and following systemd journal logs.

use anyhow::{Context as _, Result};
use std::time::{Duration, UNIX_EPOCH};
use systemd::journal::{Journal, OpenOptions};

/// Reader for systemd journal entries.
pub struct JournalReader {
    journal: Journal,
}

/// A single log entry from the journal.
pub struct LogEntry {
    /// Timestamp in microseconds since the Unix epoch.
    pub timestamp: u64,
    /// The log message.
    pub message: String,
    /// Optional service or process identifier.
    pub identifier: Option<String>,
}

impl JournalReader {
    /// Opens the journal for reading.
    pub fn new() -> Result<Self> {
        let journal = OpenOptions::default()
            .open()
            .context("Failed to open journal")?;
        Ok(Self { journal })
    }

    /// Retrieves a specified number of recent logs for a given unit.
    pub fn logs_for_unit(&mut self, unit: &str, lines: usize) -> Result<Vec<LogEntry>> {
        self.apply_unit_matches(unit)?;
        self.journal.seek_tail()?;

        let mut entries = Vec::with_capacity(lines);

        for _ in 0..lines {
            if self.journal.previous()? == 0 {
                break;
            }
            if let Some(entry) = self.read_entry() {
                entries.push(entry);
            }
        }

        entries.reverse();
        Ok(entries)
    }

    /// Follows the journal for a unit, calling the provided callback for each new entry.
    pub fn follow_unit<F>(&mut self, unit: &str, mut callback: F) -> Result<()>
    where
        F: FnMut(&LogEntry) -> bool,
    {
        // Matches should already be applied by logs_for_unit, but let's ensure it.
        // match_add is idempotent/cumulative so it doesn't hurt.
        self.apply_unit_matches(unit)?;

        // If we just called logs_for_unit, we might already be at the end.
        // But seeking to tail ensures we don't repeat what we just read.
        self.journal.seek_tail()?;

        loop {
            // Wait for new data (blocks)
            self.journal.wait(Some(Duration::from_secs(1)))?;

            while self.journal.next()? > 0 {
                if let Some(entry) = self.read_entry() {
                    if !callback(&entry) {
                        return Ok(());
                    }
                }
            }
        }
    }

    /// Reads the current journal entry into a [`LogEntry`].
    fn read_entry(&mut self) -> Option<LogEntry> {
        let message_field = self.journal.get_data("MESSAGE").ok()??;
        let message_bytes = message_field.value()?;
        let message = String::from_utf8_lossy(message_bytes).to_string();

        let timestamp = self
            .journal
            .timestamp()
            .ok()?
            .duration_since(UNIX_EPOCH)
            .ok()?
            .as_micros() as u64;

        // Try to get a meaningful identifier
        let identifier = self
            .journal
            .get_data("COMPOSE_SERVICE")
            .ok()
            .flatten()
            .and_then(|d| d.value().map(|v| String::from_utf8_lossy(v).to_string()))
            .or_else(|| {
                self.journal
                    .get_data("SYSLOG_IDENTIFIER")
                    .ok()
                    .flatten()
                    .and_then(|d| d.value().map(|v| String::from_utf8_lossy(v).to_string()))
            });

        Some(LogEntry {
            timestamp,
            message,
            identifier,
        })
    }

    fn apply_unit_matches(&mut self, unit: &str) -> Result<()> {
        let bare = unit
            .strip_prefix("compose@")
            .and_then(|s| s.strip_suffix(".service"))
            .unwrap_or(unit);

        self.journal.match_add("_SYSTEMD_UNIT", unit)?;
        self.journal.match_or()?;
        self.journal.match_add("_SYSTEMD_USER_UNIT", unit)?;
        self.journal.match_or()?;
        self.journal.match_add("UNIT", unit)?;
        self.journal.match_or()?;
        self.journal.match_add("USER_UNIT", unit)?;
        self.journal.match_or()?;
        self.journal.match_add("COMPOSE_PROJECT", bare)?;
        Ok(())
    }
}
