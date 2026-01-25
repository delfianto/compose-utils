//! Core application infrastructure.
//!
//! This module contains fundamental types and utilities used throughout
//! the application, including runtime context, constants, and validation.

pub mod constants;
pub mod context;
pub mod validation;

pub use constants::*;
pub use context::{get_context, read_env_file, Context};
pub use validation::*;
