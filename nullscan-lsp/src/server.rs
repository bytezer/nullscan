//! The document store, the custom request handlers and the summary cache.
//!
//! Single-document analysis stays synchronous; workspace scans run on a worker thread and
//! come back through a job channel, so this module owns every piece of server state and
//! the dispatch loop only routes messages.

use std::collections::{HashMap, HashSet};
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};

use crossbeam_channel::{Receiver, Sender, unbounded};
use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CancelParams, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, InitializeParams, MessageType,
    NumberOrString, Uri,
    notification::{
        Cancel, DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument,
        DidSaveTextDocument, Notification as _,
    },
};
use nullscan_core::{Config, Language, analyze};
use nullscan_workspace::config::load_in_dir;
use nullscan_workspace::json::summary_json;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::convert;
use crate::messages::{self, MessageSink, log_message};
use crate::requests::{self, AnalyzeWorkspaceParams, JobResult, ScanJob};
use crate::uri::{from_file_path, to_file_path};

/// One open buffer. Full sync keeps the whole text, so no incremental state is stored.
pub struct Document {
    text: String,
    version: i32,
}

/// Owns every open document, the workspace folders, the in-flight scans and the summary.
pub struct Server {
    sink: MessageSink,
    job_sender: Sender<JobResult>,
    job_receiver: Receiver<JobResult>,
    documents: HashMap<Uri, Document>,
    folders: Vec<PathBuf>,
    /// URIs this server has pushed diagnostics for, so a later scan can clear them.
    published: HashSet<Uri>,
    /// At most one workspace scan runs at a time; the flag is what `$/cancelRequest` sets.
    in_flight: HashMap<lsp_server::RequestId, Arc<AtomicBool>>,
    summary: Option<Value>,
    summary_stale: bool,
}

impl Server {
    pub fn new(sink: MessageSink, folders: Vec<PathBuf>) -> Self {
        let (job_sender, job_receiver) = unbounded();
        Self {
            sink,
            job_sender,
            job_receiver,
            documents: HashMap::new(),
            folders,
            published: HashSet::new(),
            in_flight: HashMap::new(),
            summary: None,
            summary_stale: false,
        }
    }

    /// A clone of the job channel, so the dispatch loop can `select!` on it without
    /// borrowing the server while it also mutates it.
    pub fn job_results(&self) -> Receiver<JobResult> {
        self.job_receiver.clone()
    }

    /// Routes a client notification; unknown methods are ignored.
    pub fn handle_notification(&mut self, notification: &Notification) {
        match notification.method.as_str() {
            DidOpenTextDocument::METHOD => {
                if let Some(params) = params_of::<DidOpenTextDocument>(notification) {
                    self.did_open(params);
                }
            }
            DidChangeTextDocument::METHOD => {
                if let Some(params) = params_of::<DidChangeTextDocument>(notification) {
                    self.did_change(params);
                }
            }
            DidSaveTextDocument::METHOD => {
                if let Some(params) = params_of::<DidSaveTextDocument>(notification) {
                    self.did_save(params);
                }
            }
            DidCloseTextDocument::METHOD => {
                if let Some(params) = params_of::<DidCloseTextDocument>(notification) {
                    self.did_close(params);
                }
            }
            Cancel::METHOD => {
                if let Some(params) = params_of::<Cancel>(notification) {
                    self.cancel_request(params);
                }
            }
            other => log::debug!("ignoring notification {other}"),
        }
    }

    /// Handles the custom requests. Returns `false` for any other method, so the caller
    /// can answer with `MethodNotFound`.
    pub fn handle_request(&mut self, request: &Request) -> bool {
        match request.method.as_str() {
            requests::ANALYZE_WORKSPACE_METHOD => self.analyze_workspace(request),
            requests::SUMMARY_METHOD => self.summary_response(request),
            _ => return false,
        }
        true
    }

    /// Applies a finished worker job on the dispatch thread: log, publish, cache, reply.
    pub fn finish_job(&mut self, job: JobResult) {
        self.in_flight.remove(&job.request_id);
        let report = job.report;
        for warning in &report.warnings {
            log_message(&self.sink, MessageType::WARNING, warning);
        }
        if report.cancelled {
            let message = "workspace scan was cancelled".to_owned();
            return self.reply_error(job.request_id, ErrorCode::RequestCanceled, message);
        }

        let summary = summary_json(&report);
        self.publish_report(&report);
        self.summary = Some(summary.clone());
        self.summary_stale = false;
        self.reply_ok(job.request_id, summary);
    }

    fn did_open(&mut self, params: DidOpenTextDocumentParams) {
        let item = params.text_document;
        self.documents.insert(
            item.uri.clone(),
            Document {
                text: item.text,
                version: item.version,
            },
        );
        self.summary_stale = true;
        self.publish_for_document(&item.uri);
    }

    /// Full sync: only the last content change matters and carries the complete text.
    fn did_change(&mut self, params: DidChangeTextDocumentParams) {
        self.summary_stale = true;
        let uri = params.text_document.uri;
        let Some(change) = params.content_changes.into_iter().next_back() else {
            return;
        };
        let Some(document) = self.documents.get_mut(&uri) else {
            log::debug!("ignoring change for unopened document {}", uri.as_str());
            return;
        };
        document.text = change.text;
        document.version = params.text_document.version;
    }

    fn did_save(&mut self, params: DidSaveTextDocumentParams) {
        self.summary_stale = true;
        self.publish_for_document(&params.text_document.uri);
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.summary_stale = true;
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.publish(&uri, Vec::new(), None);
    }

    /// Starts one workspace scan, or answers with an error when one is already running.
    fn analyze_workspace(&mut self, request: &Request) {
        let id = request.id.clone();
        let params = match parse_params::<AnalyzeWorkspaceParams>(&request.params) {
            Ok(params) => params,
            Err(message) => return self.reply_error(id, ErrorCode::InvalidParams, message),
        };
        let folders = match self.scan_folders(params.folders) {
            Ok(folders) => folders,
            Err(message) => return self.reply_error(id, ErrorCode::InvalidParams, message),
        };
        if !self.in_flight.is_empty() {
            let message = "a workspace scan is already running".to_owned();
            return self.reply_error(id, ErrorCode::RequestFailed, message);
        }

        let cancel = Arc::new(AtomicBool::new(false));
        self.in_flight.insert(id.clone(), Arc::clone(&cancel));
        let job = ScanJob {
            request_id: id,
            folders,
            overrides: self.overrides(),
            cancel,
            sink: Arc::clone(&self.sink),
        };
        requests::spawn(job, self.job_sender.clone());
    }

    /// Answers from the cache; `stale` says the buffers changed since the last scan.
    fn summary_response(&mut self, request: &Request) {
        let result = serde_json::json!({
            "summary": self.summary,
            "stale": self.summary_stale,
        });
        self.reply_ok(request.id.clone(), result);
    }

    /// Explicit request folders win; otherwise every folder from `initialize`.
    fn scan_folders(&self, folders: Option<Vec<Uri>>) -> Result<Vec<PathBuf>, String> {
        let Some(folders) = folders.filter(|folders| !folders.is_empty()) else {
            return Ok(self.folders.clone());
        };
        folders
            .iter()
            .map(|uri| to_file_path(uri).ok_or_else(|| format!("not a file URI: {}", uri.as_str())))
            .collect()
    }

    /// Dirty buffers win over disk, keyed by the path the walker will produce.
    fn overrides(&self) -> HashMap<PathBuf, String> {
        self.documents
            .iter()
            .filter_map(|(uri, document)| {
                to_file_path(uri).map(|path| (path, document.text.clone()))
            })
            .collect()
    }

    fn cancel_request(&mut self, params: CancelParams) {
        let id = request_id(params.id);
        match self.in_flight.get(&id) {
            Some(flag) => {
                log::info!("cancelling workspace scan {id}");
                flag.store(true, Ordering::Relaxed);
            }
            None => log::debug!("ignoring cancel for unknown request {id}"),
        }
    }

    /// Publishes one notification per scanned file and clears URIs that disappeared.
    fn publish_report(&mut self, report: &nullscan_workspace::ScanReport) {
        let mut current = HashSet::new();
        for file in &report.files {
            let uri = from_file_path(&file.file);
            let diagnostics = file
                .diagnostics
                .iter()
                .map(|diagnostic| convert::to_lsp_diagnostic(&file.source, diagnostic))
                .collect();
            self.publish(&uri, diagnostics, None);
            current.insert(uri);
        }

        let gone: Vec<Uri> = self.published.difference(&current).cloned().collect();
        for uri in gone {
            self.publish(&uri, Vec::new(), None);
        }
        self.published = current;
    }

    /// Analyzes the stored buffer and publishes its diagnostics.
    fn publish_for_document(&mut self, uri: &Uri) {
        let Some(document) = self.documents.get(uri) else {
            return;
        };
        let diagnostics = match language_for(uri) {
            Some(language) => self.analyze_document(uri, &document.text, language),
            None => {
                log::debug!("skipping unsupported document {}", uri.as_str());
                Vec::new()
            }
        };
        let version = Some(document.version);
        self.publish(uri, diagnostics, version);
    }

    fn analyze_document(
        &self,
        uri: &Uri,
        text: &str,
        language: Language,
    ) -> Vec<lsp_types::Diagnostic> {
        let config = self.config_for(uri);
        analyze(text, language, &config)
            .iter()
            .map(|diagnostic| convert::to_lsp_diagnostic(text, diagnostic))
            .collect()
    }

    /// Resolves the nearest workspace folder's `nullscan.toml`, falling back to defaults.
    fn config_for(&self, uri: &Uri) -> Config {
        let Some(path) = to_file_path(uri) else {
            return Config::default();
        };
        let Some(dir) = self.config_dir(&path) else {
            return Config::default();
        };
        match load_in_dir(dir) {
            Ok(loaded) => {
                for warning in &loaded.warnings {
                    log::warn!("{warning}");
                }
                loaded.config
            }
            Err(error) => {
                log_message(
                    &self.sink,
                    MessageType::WARNING,
                    &format!("nullscan config: {error}"),
                );
                Config::default()
            }
        }
    }

    /// The longest workspace folder containing `path`, else the file's own directory.
    fn config_dir<'a>(&'a self, path: &'a Path) -> Option<&'a Path> {
        let folder = self
            .folders
            .iter()
            .filter(|folder| path.starts_with(folder))
            .max_by_key(|folder| folder.as_os_str().len());
        folder.map(PathBuf::as_path).or_else(|| path.parent())
    }

    fn publish(
        &mut self,
        uri: &Uri,
        diagnostics: Vec<lsp_types::Diagnostic>,
        version: Option<i32>,
    ) {
        messages::publish(&self.sink, uri, diagnostics, version);
        self.published.insert(uri.clone());
    }

    fn reply_ok(&self, id: lsp_server::RequestId, result: Value) {
        let response = Response::new_ok(id, result);
        (self.sink)(Message::Response(response));
    }

    fn reply_error(&self, id: lsp_server::RequestId, code: ErrorCode, message: String) {
        log::warn!("{message}");
        let response = Response::new_err(id, code as i32, message);
        (self.sink)(Message::Response(response));
    }
}

/// Collects workspace folder paths from `initialize`, preferring `workspace_folders` over
/// the older single `root_uri`.
#[allow(deprecated)]
pub fn workspace_folders(params: &InitializeParams) -> Vec<PathBuf> {
    let folders: Vec<PathBuf> = params
        .workspace_folders
        .as_deref()
        .unwrap_or(&[])
        .iter()
        .filter_map(|folder| to_file_path(&folder.uri))
        .collect();
    if !folders.is_empty() {
        return folders;
    }

    params
        .root_uri
        .as_ref()
        .and_then(|uri| to_file_path(uri))
        .into_iter()
        .collect()
}

/// Parses a request payload, treating a missing or `null` body as the default value.
fn parse_params<T: DeserializeOwned + Default>(params: &Value) -> Result<T, String> {
    if params.is_null() {
        return Ok(T::default());
    }
    serde_json::from_value(params.clone()).map_err(|error| format!("invalid params: {error}"))
}

fn request_id(id: NumberOrString) -> lsp_server::RequestId {
    match id {
        NumberOrString::Number(number) => lsp_server::RequestId::from(number),
        NumberOrString::String(text) => lsp_server::RequestId::from(text),
    }
}

/// Parses a notification's params, logging a malformed payload instead of failing.
fn params_of<N: lsp_types::notification::Notification>(
    notification: &Notification,
) -> Option<N::Params> {
    match serde_json::from_value(notification.params.clone()) {
        Ok(params) => Some(params),
        Err(error) => {
            log::warn!("invalid params for {}: {error}", notification.method);
            None
        }
    }
}

fn language_for(uri: &Uri) -> Option<Language> {
    let path = to_file_path(uri)?;
    Language::from_path(&path)
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::sync::{Arc, Mutex};

    use lsp_server::{RequestId, Response};
    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, VersionedTextDocumentIdentifier,
        notification::PublishDiagnostics,
    };
    use serde_json::{Value, json};

    fn document_uri() -> Uri {
        "file:///tmp/fixture/a.ts".parse().expect("valid uri")
    }

    fn server() -> (Server, Arc<Mutex<Vec<Message>>>) {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink_messages = Arc::clone(&messages);
        let sink: MessageSink = Arc::new(move |message| {
            sink_messages.lock().expect("lock messages").push(message);
        });
        (Server::new(sink, vec![PathBuf::from("/tmp/fixture")]), messages)
    }

    fn published(messages: &Arc<Mutex<Vec<Message>>>) -> Vec<lsp_types::PublishDiagnosticsParams> {
        messages
            .lock()
            .expect("lock messages")
            .iter()
            .filter_map(|message| match message {
                Message::Notification(notification)
                    if notification.method == PublishDiagnostics::METHOD =>
                {
                    serde_json::from_value(notification.params.clone()).ok()
                }
                _ => None,
            })
            .collect()
    }

    fn response_for(
        messages: &Arc<Mutex<Vec<Message>>>,
        id: i32,
    ) -> Result<Value, lsp_server::ResponseError> {
        let want = RequestId::from(id);
        let messages = messages.lock().expect("lock messages");
        let response: Response = messages
            .iter()
            .find_map(|message| match message {
                Message::Response(response) if response.id == want => Some(response.clone()),
                _ => None,
            })
            .expect("response");
        response.response_result
    }

    fn request(id: i32, method: &str, params: Value) -> Request {
        Request {
            id: RequestId::from(id),
            method: method.to_owned(),
            params,
        }
    }

    fn open(server: &mut Server) {
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: document_uri(),
                language_id: "typescript".to_owned(),
                version: 1,
                text: "console.log(x);\n".to_owned(),
            },
        });
    }

    #[test]
    fn change_keeps_the_buffer_in_sync_and_save_publishes_it() {
        let (mut server, messages) = server();
        open(&mut server);
        assert_eq!(published(&messages).len(), 1);

        server.did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: document_uri(),
                version: 2,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: "const x = 1;\n".to_owned(),
            }],
        });
        // didChange only updates the store; analysis runs on save.
        assert_eq!(published(&messages).len(), 1);

        server.did_save(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: document_uri() },
            text: None,
        });
        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert!(publishes[1].diagnostics.is_empty());
        assert_eq!(publishes[1].version, Some(2));
    }

    #[test]
    fn close_removes_the_document_and_clears_diagnostics() {
        let (mut server, messages) = server();
        open(&mut server);
        assert!(!published(&messages)[0].diagnostics.is_empty());

        server.did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: document_uri() },
        });

        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert!(publishes[1].diagnostics.is_empty());
        assert_eq!(publishes[1].version, None);
        assert!(server.documents.is_empty());
    }

    #[test]
    fn summary_is_empty_and_fresh_before_any_edit() {
        let (mut server, messages) = server();

        assert!(server.handle_request(&request(7, requests::SUMMARY_METHOD, Value::Null)));

        let result = response_for(&messages, 7).expect("ok response");
        assert_eq!(result["summary"], Value::Null);
        assert_eq!(result["stale"], false);
    }

    #[test]
    fn any_document_notification_marks_the_summary_stale() {
        let (mut server, messages) = server();
        open(&mut server);

        assert!(server.handle_request(&request(8, requests::SUMMARY_METHOD, Value::Null)));

        let result = response_for(&messages, 8).expect("ok response");
        assert_eq!(result["stale"], true);
    }

    #[test]
    fn unknown_requests_are_left_to_the_dispatch_loop() {
        let (mut server, _messages) = server();

        assert!(!server.handle_request(&request(9, "textDocument/hover", json!({}))));
    }

    #[test]
    fn a_non_file_folder_is_an_invalid_params_error() {
        let (mut server, messages) = server();

        assert!(server.handle_request(&request(
            10,
            requests::ANALYZE_WORKSPACE_METHOD,
            json!({ "folders": ["https://example.com/repo"] })
        )));

        let error = response_for(&messages, 10).expect_err("error response");
        assert_eq!(error.code, ErrorCode::InvalidParams as i32);
        assert!(server.in_flight.is_empty());
    }

    #[test]
    fn a_scan_clears_diagnostics_of_files_that_disappeared() {
        let (mut server, messages) = server();
        open(&mut server);

        server.publish_report(&nullscan_workspace::ScanReport {
            files: Vec::new(),
            duplicates: Vec::new(),
            warnings: Vec::new(),
            duration: std::time::Duration::ZERO,
            truncated: false,
            cancelled: false,
            config_path: None,
            disabled_rules: Vec::new(),
        });

        let last = published(&messages).pop().expect("clearing publish");
        assert_eq!(last.uri, document_uri());
        assert!(last.diagnostics.is_empty());
        assert_eq!(last.version, None);
    }

    #[test]
    fn request_folders_override_the_initialized_folders() {
        let (server, _messages) = server();

        let explicit = server
            .scan_folders(Some(vec!["file:///other/repo".parse().expect("uri")]))
            .expect("explicit folders");
        assert_eq!(explicit, vec![PathBuf::from("/other/repo")]);

        let fallback = server.scan_folders(None).expect("initialized folders");
        assert_eq!(fallback, vec![PathBuf::from("/tmp/fixture")]);
    }

    #[test]
    fn cancel_sets_the_running_scan_flag() {
        let (mut server, _messages) = server();
        let flag = Arc::new(AtomicBool::new(false));
        server.in_flight.insert(RequestId::from(5), Arc::clone(&flag));

        let notification = Notification::new(
            Cancel::METHOD.to_owned(),
            CancelParams {
                id: NumberOrString::Number(5),
            },
        );
        server.handle_notification(&notification);

        assert!(flag.load(Ordering::Relaxed));
    }

    #[test]
    fn only_one_workspace_scan_runs_at_a_time() {
        let (mut server, messages) = server();
        server
            .in_flight
            .insert(RequestId::from(11), Arc::new(AtomicBool::new(false)));

        assert!(server.handle_request(&request(
            12,
            requests::ANALYZE_WORKSPACE_METHOD,
            Value::Null
        )));

        let error = response_for(&messages, 12).expect_err("error response");
        assert_eq!(error.code, ErrorCode::RequestFailed as i32);
        assert_eq!(error.message, "a workspace scan is already running");
    }
}
