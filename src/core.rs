//! Core application infrastructure.
//!
//! This module contains fundamental types and utilities used throughout
//! the application, including runtime context, constants, and validation.

pub mod constants;
pub mod context;
pub mod output;
pub mod validation;
pub mod verbose;

pub use constants::*;
pub use context::{Context, get_context, read_env_file, should_use_infisical};
pub use output::{Report, enable as enable_json, is_enabled as is_json, print_json};
pub use validation::*;
pub use verbose::enable as enable_verbose;
