//! Module for interfacing with systemd.
//!
//! Provides abstractions for service discovery, management of unit overrides,
//! and invocation of `systemctl`.

pub mod discovery;
pub mod manager;
pub mod service;
