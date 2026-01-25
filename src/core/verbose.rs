//! Verbose/debug output support.
//!
//! This module provides a global verbose flag and a macro for conditional debug output.

use std::sync::atomic::{AtomicBool, Ordering};

/// Global flag indicating whether verbose output is enabled.
static VERBOSE: AtomicBool = AtomicBool::new(false);

/// Enables verbose output mode.
pub fn enable() {
    VERBOSE.store(true, Ordering::SeqCst);
}

/// Returns whether verbose output is currently enabled.
pub fn is_enabled() -> bool {
    VERBOSE.load(Ordering::SeqCst)
}

/// Prints a debug message to stderr if verbose mode is enabled.
///
/// Usage: `verbose!("message")` or `verbose!("format {}", arg)`
#[macro_export]
macro_rules! verbose {
    ($($arg:tt)*) => {
        if $crate::core::verbose::is_enabled() {
            eprintln!("[debug] {}", format!($($arg)*));
        }
    };
}
