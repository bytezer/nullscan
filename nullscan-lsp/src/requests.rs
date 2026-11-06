//! The custom requests and the worker job that answers them.
//!
//! A workspace scan walks every folder, loads that folder's own `nullscan.toml`, merges
//! the per-folder reports and sends the result back to the dispatch thread. The worker
//! owns a snapshot (folders, overrides, cancellation flag and a sink clone) and never
//! touches [`crate::server::Server`], so all server state stays on one thread.

use std::collections::HashMap;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::Sender;
use lsp_server::RequestId;
use lsp_types::MessageType;
use nullscan_core::Config;
use nullscan_workspace::{ScanOptions, ScanReport, config, merge, scan};
use serde::Deserialize;

use crate::messages::{MessageSink, log_message};

pub const ANALYZE_WORKSPACE_METHOD: &str = "nullscan/analyzeWorkspace";
pub const SUMMARY_METHOD: &str = "nullscan/summary";

/// Registered as project-scoped in `nullscan-core`, so it is only disabled for a scan and
/// never reaches the per-file rule set.
const DUPLICATE_CODE_RULE: &str = "duplicate-code";

/// Optional `{"folders": ["file:///..."]}` payload; omitted means every workspace folder.
#[derive(Deserialize, Default)]
pub struct AnalyzeWorkspaceParams {
    #[serde(default)]
    pub folders: Option<Vec<lsp_types::Uri>>,
}

/// Everything the worker needs, taken as a snapshot so it cannot observe later edits.
pub struct ScanJob {
    pub request_id: RequestId,
    pub folders: Vec<PathBuf>,
    /// `--config`: every folder is scanned with this file instead of its own `nullscan.toml`.
    pub config_path: Option<PathBuf>,
    pub overrides: HashMap<PathBuf, String>,
    pub max_diagnostics: usize,
    pub include_duplicates: bool,
    pub cancel: Arc<AtomicBool>,
    pub sink: MessageSink,
}

/// What the worker sends back; the dispatch thread applies it and answers the request.
pub struct JobResult {
    pub request_id: RequestId,
    pub report: ScanReport,
}

/// Runs `job` on its own thread. A failed send only means the server is shutting down.
pub fn spawn(job: ScanJob, results: Sender<JobResult>) {
    std::thread::spawn(move || {
        let request_id = job.request_id.clone();
        let report = run(&job);
        let _ = results.send(JobResult { request_id, report });
    });
}

/// Scans each folder with its own config and merges the reports into one view.
fn run(job: &ScanJob) -> ScanReport {
    let mut reports = Vec::with_capacity(job.folders.len());
    let mut stopped_between_folders = false;

    for folder in &job.folders {
        if job.cancel.load(Ordering::Relaxed) {
            stopped_between_folders = true;
            break;
        }
        reports.push(scan_folder(job, folder));
    }

    let mut merged = merge(reports);
    merged.cancelled |= stopped_between_folders;
    merged
}

fn scan_folder(job: &ScanJob, folder: &Path) -> ScanReport {
    let (mut config, config_path) = folder_config(job, folder);
    if !job.include_duplicates {
        // `Config` has no `Clone`; the loader already returns it owned, so mutating it here
        // keeps `nullscan-workspace`'s API untouched.
        config.disabled_rules.insert(DUPLICATE_CODE_RULE.to_owned());
    }
    let paths = [folder.to_path_buf()];
    scan(ScanOptions {
        paths: &paths,
        config,
        config_path,
        max_diagnostics: Some(job.max_diagnostics),
        overrides: Some(&job.overrides),
        cancel: Some(&job.cancel),
    })
}

/// An explicit `--config` wins over the folder's own file. A broken or missing config is
/// reported and the folder is scanned with defaults, so one bad file never hides every
/// diagnostic of a workspace.
fn folder_config(job: &ScanJob, folder: &Path) -> (Config, Option<PathBuf>) {
    let loaded = match &job.config_path {
        Some(path) => config::load(Some(path)),
        None => config::load_in_dir(folder),
    };
    match loaded {
        Ok(loaded) => {
            for warning in &loaded.warnings {
                log_message(&job.sink, MessageType::WARNING, warning);
            }
            (loaded.config, loaded.path)
        }
        Err(error) => {
            let message = format!("nullscan config: {error}");
            log::warn!("{message}");
            log_message(&job.sink, MessageType::WARNING, &message);
            (Config::default(), None)
        }
    }
}
