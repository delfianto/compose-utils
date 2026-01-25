//! Logic for reading and following systemd journal logs.

use anyhow::{Context as _, Result};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::Arc;
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
    ///
    /// The follow loop will exit gracefully when:
    /// - The callback returns `false`
    /// - The user presses Ctrl+C (SIGINT)
    ///
    /// # Arguments
    ///
    /// * `unit` - The systemd unit to follow logs for.
    /// * `callback` - A function called for each new log entry. Return `false` to stop following.
    pub fn follow_unit<F>(&mut self, unit: &str, mut callback: F) -> Result<()>
    where
        F: FnMut(&LogEntry) -> bool,
    {
        let interrupted = Arc::new(AtomicBool::new(false));
        let interrupted_clone = Arc::clone(&interrupted);

        ctrlc::set_handler(move || {
            interrupted_clone.store(true, Ordering::SeqCst);
        })
        .context("Failed to set Ctrl+C handler")?;

        self.apply_unit_matches(unit)?;

        // Ensure we don't repeat what we just read if logs_for_unit was called prior.
        self.journal.seek_tail()?;

        loop {
            if interrupted.load(Ordering::SeqCst) {
                return Ok(());
            }

            // Block for up to 1 second waiting for new data.
            self.journal.wait(Some(Duration::from_secs(1)))?;

            if interrupted.load(Ordering::SeqCst) {
                return Ok(());
            }

            while self.journal.next()? > 0 {
                if let Some(entry) = self.read_entry() {
                    if !callback(&entry) {
                        return Ok(());
                    }
                }

                if interrupted.load(Ordering::SeqCst) {
                    return Ok(());
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

        // Try to obtain a meaningful identifier from COMPOSE_SERVICE or SYSLOG_IDENTIFIER.
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
