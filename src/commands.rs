//! High-level command implementations for the application.
//!
//! Each submodule corresponds to a primary CLI action and contains the
//! logic for service discovery, validation, and execution.

pub mod compose_direct;
pub mod config;
pub mod deps;
pub mod internal;
pub mod ps;
pub mod pull;
pub mod service;
pub mod update;

pub use compose_direct::{compose_down, compose_restart, compose_up};
pub use pull::run_pull;
pub use service::{
    run_disable, run_enable, run_restart, run_start, run_status, run_stop, run_sync,
};
pub use update::run_update;
