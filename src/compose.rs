//! Module for parsing and managing Docker Compose projects.
//!
//! Provides utilities for reading `.env` files, discovering project
//! structures, and extracting image definitions.

pub mod dependencies;
pub mod env;
pub mod project;
pub mod types;
