//! Workspace-level orchestration shared by the CLI and the editor integration.
//!
//! This crate owns the parts that must behave identically everywhere: which files are
//! scanned, how `nullscan.toml` becomes a [`nullscan_core::Config`], how a full scan is
//! scheduled, and how its results serialize. It deliberately has no `clap` or LSP
//! dependency so both binaries can depend on it without pulling in each other's stack.

pub mod config;
pub mod ignore;
pub mod json;
pub mod modules;
pub mod scan;
pub mod walk;

pub use scan::{FileResult, ScanOptions, ScanReport, merge, render_json, scan};
