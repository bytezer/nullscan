//! The document store, the custom request handlers and the summary cache.
//!
//! Single-document analysis stays synchronous; workspace scans run on a worker thread and
//! come back through a job channel, so this module owns every piece of server state and
//! the dispatch loop only routes messages.

use std::collections::HashMap;
use std::ffi::OsStr;
use std::path::{Path, PathBuf};
use std::sync::Arc;
use std::sync::atomic::{AtomicBool, Ordering};
#[cfg(test)]
use std::time::Duration;

use crossbeam_channel::{Receiver, Sender, unbounded};
use lsp_server::{ErrorCode, Message, Notification, Request, Response};
use lsp_types::{
    CancelParams, DidChangeConfigurationParams, DidChangeTextDocumentParams,
    DidChangeWatchedFilesParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
    DidSaveTextDocumentParams, HoverParams, InitializeParams, MessageType, NumberOrString, Uri,
    notification::{
        Cancel, DidChangeConfiguration, DidChangeTextDocument, DidChangeWatchedFiles,
        DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument, Notification as _,
    },
    request::{HoverRequest, Request as _},
};
use nullscan_core::{Config, Language, analyze};
use nullscan_workspace::config::load_in_dir;
use nullscan_workspace::json::summary_json;
use serde::de::DeserializeOwned;
use serde_json::Value;

use crate::convert;
use crate::debounce::Debouncer;
use crate::hover;
use crate::messages::{self, MessageSink, log_message};
use crate::requests::{self, AnalyzeWorkspaceParams, JobResult, ScanJob};
use crate::settings::Settings;
use crate::uri::{from_file_path, to_file_path};

/// The file name whose change makes the open documents re-analyze under a new rule set.
const CONFIG_FILE_NAME: &str = "nullscan.toml";

/// One open buffer. Full sync keeps the whole text, so no incremental state is stored.
pub struct Document {
    text: String,
    version: i32,
}

/// Diagnostics published for one URI, split by their lifetime.
///
/// The file group follows the buffer (open, save, type, scan); the duplicates group comes
/// from the last workspace scan and an edit invalidates it because its ranges belong to
/// the text that was scanned.
#[derive(Default)]
struct Published {
    file: Vec<lsp_types::Diagnostic>,
    duplicates: Vec<lsp_types::Diagnostic>,
}

impl Published {
    fn combined(&self) -> Vec<lsp_types::Diagnostic> {
        self.file
            .iter()
            .chain(&self.duplicates)
            .cloned()
            .collect()
    }
}

/// Owns every open document, the workspace folders, the in-flight scans and the summary.
pub struct Server {
    sink: MessageSink,
    job_sender: Sender<JobResult>,
    job_receiver: Receiver<JobResult>,
    debouncer: Debouncer,
    settings: Settings,
    documents: HashMap<Uri, Document>,
    folders: Vec<PathBuf>,
    /// Everything currently published per URI, so an edit can drop only the duplicates
    /// and a scan can clear the files that disappeared.
    published: HashMap<Uri, Published>,
    /// At most one workspace scan runs at a time; the flag is what `$/cancelRequest` sets.
    in_flight: HashMap<lsp_server::RequestId, Arc<AtomicBool>>,
    summary: Option<Value>,
    summary_stale: bool,
}

impl Server {
    pub fn new(sink: MessageSink, folders: Vec<PathBuf>, settings: Settings) -> Self {
        Self::assemble(sink, folders, settings, Debouncer::new())
    }

    /// The debounce interval is injectable so unit tests do not wait the production 250 ms.
    #[cfg(test)]
    fn with_debounce_interval(
        sink: MessageSink,
        folders: Vec<PathBuf>,
        settings: Settings,
        interval: Duration,
    ) -> Self {
        Self::assemble(sink, folders, settings, Debouncer::with_interval(interval))
    }

    fn assemble(
        sink: MessageSink,
        folders: Vec<PathBuf>,
        settings: Settings,
        debouncer: Debouncer,
    ) -> Self {
        let (job_sender, job_receiver) = unbounded();
        Self {
            sink,
            job_sender,
            job_receiver,
            debouncer,
            settings,
            documents: HashMap::new(),
            folders,
            published: HashMap::new(),
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

    /// A clone of the debounced-edit channel, for the same reason as [`Self::job_results`].
    pub fn debounce_results(&self) -> Receiver<Uri> {
        self.debouncer.results()
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
            DidChangeConfiguration::METHOD => {
                if let Some(params) = params_of::<DidChangeConfiguration>(notification) {
                    self.did_change_configuration(params);
                }
            }
            DidChangeWatchedFiles::METHOD => {
                if let Some(params) = params_of::<DidChangeWatchedFiles>(notification) {
                    self.did_change_watched_files(params);
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
            HoverRequest::METHOD => self.hover_request(request),
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
        if self.settings.analyze_on_open {
            self.publish_for_document(&item.uri);
        }
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
        // The scan ranges and the file ranges no longer describe this text, but only the
        // duplicates are whole-project data; the file group is refreshed on type/save.
        self.drop_duplicate_diagnostics(&uri);
        if self.settings.analyze_on_type {
            self.debouncer.schedule(uri);
        }
    }

    fn did_save(&mut self, params: DidSaveTextDocumentParams) {
        self.summary_stale = true;
        let uri = params.text_document.uri;
        self.drop_duplicate_diagnostics(&uri);
        if self.settings.analyze_on_save {
            self.publish_saved_document(&uri);
        }
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        self.summary_stale = true;
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        // Any queued analyze-on-type work for `uri` is dropped in [`Self::analyze_debounced`].
        self.publish(&uri, Published::default(), None);
        self.published.remove(&uri);
    }

    /// New settings arrive as the whole `nullscan` section from the client.
    fn did_change_configuration(&mut self, params: DidChangeConfigurationParams) {
        self.settings.apply(&params.settings);
        log::info!("settings: {:?}", self.settings);
        self.summary_stale = true;
        if self.settings.analyze_on_open {
            self.republish_open_documents();
        }
    }

    /// The client owns watcher registration; the server only reacts to the notification it
    /// is sent. A `nullscan.toml` change republishes the open buffers under the new rule
    /// set, while any other watched file is irrelevant here.
    fn did_change_watched_files(&mut self, params: DidChangeWatchedFilesParams) {
        let mut config_changed = false;
        for change in &params.changes {
            if self.is_config_change(&change.uri) {
                config_changed = true;
            }
        }
        if !config_changed {
            return;
        }
        log::info!("nullscan.toml changed; republishing open documents");
        self.summary_stale = true;
        self.republish_open_documents();
    }

    fn is_config_change(&self, uri: &Uri) -> bool {
        let Some(path) = to_file_path(uri) else {
            log::debug!("ignoring non-file watched URI {}", uri.as_str());
            return false;
        };
        if path.file_name() == Some(OsStr::new(CONFIG_FILE_NAME)) {
            return true;
        }
        log::debug!("ignoring watched file {}", path.display());
        false
    }

    /// Applies one debounced analyze-on-type fire.
    ///
    /// Checking that the URI is still open is what makes `didClose` cancel pending work,
    /// and reading the stored buffer here is what makes the last edit win.
    pub fn analyze_debounced(&mut self, uri: Uri) {
        if !self.settings.analyze_on_type {
            return;
        }
        if !self.documents.contains_key(&uri) {
            log::debug!(
                "dropping debounced analysis for closed document {}",
                uri.as_str()
            );
            return;
        }
        self.publish_for_document(&uri);
    }

    fn republish_open_documents(&mut self) {
        let uris: Vec<Uri> = self.documents.keys().cloned().collect();
        for uri in uris {
            self.publish_for_document(&uri);
        }
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
            max_diagnostics: self.settings.max_diagnostics,
            include_duplicates: self.settings.include_duplicates,
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
    ///
    /// File-scoped diagnostics fill the file group and every duplicate fills the
    /// duplicates group of both of its sides.
    fn publish_report(&mut self, report: &nullscan_workspace::ScanReport) {
        let mut duplicates = convert::duplicate_diagnostics(report);
        let mut current: HashMap<Uri, Published> = HashMap::new();
        for file in &report.files {
            let uri = from_file_path(&file.file);
            let published = Published {
                file: file
                    .diagnostics
                    .iter()
                    .map(|diagnostic| convert::to_lsp_diagnostic(&file.source, diagnostic))
                    .collect(),
                duplicates: duplicates.remove(&file.file).unwrap_or_default(),
            };
            self.send(&uri, &published, None);
            current.insert(uri, published);
        }

        let gone: Vec<Uri> = self
            .published
            .keys()
            .filter(|uri| !current.contains_key(*uri))
            .cloned()
            .collect();
        for uri in gone {
            self.publish(&uri, Published::default(), None);
        }
        self.published = current;
    }

    /// Analyzes the stored buffer and publishes its diagnostics, keeping the duplicates
    /// the last workspace scan found.
    fn publish_for_document(&mut self, uri: &Uri) {
        let Some(document) = self.documents.get(uri) else {
            return;
        };
        let version = Some(document.version);
        let file = self.analyze_document(uri, &document.text);
        let duplicates = self.duplicates_for(uri);
        self.publish(uri, Published { file, duplicates }, version);
    }

    /// Analyzes the stored buffer after a save, when the scan duplicates are stale too.
    fn publish_saved_document(&mut self, uri: &Uri) {
        let Some(document) = self.documents.get(uri) else {
            return;
        };
        let version = Some(document.version);
        let file = self.analyze_document(uri, &document.text);
        self.publish(
            uri,
            Published {
                file,
                duplicates: Vec::new(),
            },
            version,
        );
    }

    fn duplicates_for(&self, uri: &Uri) -> Vec<lsp_types::Diagnostic> {
        self.published
            .get(uri)
            .map(|published| published.duplicates.clone())
            .unwrap_or_default()
    }

    /// Drops the duplicates of a just-edited buffer: their ranges describe the text of the
    /// last workspace scan. The next scan restores them for the new content.
    fn drop_duplicate_diagnostics(&mut self, uri: &Uri) {
        let Some(published) = self.published.get(uri) else {
            return;
        };
        if published.duplicates.is_empty() {
            return;
        }
        let file = published.file.clone();
        let version = self.documents.get(uri).map(|document| document.version);
        self.publish(
            uri,
            Published {
                file,
                duplicates: Vec::new(),
            },
            version,
        );
    }

    /// Analyzes one buffer; unsupported documents publish an empty file group.
    fn analyze_document(&self, uri: &Uri, text: &str) -> Vec<lsp_types::Diagnostic> {
        let Some(language) = language_for(uri) else {
            log::debug!("skipping unsupported document {}", uri.as_str());
            return Vec::new();
        };
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

    /// Answers `textDocument/hover` from the diagnostics currently published for the URI.
    fn hover_request(&mut self, request: &Request) {
        let id = request.id.clone();
        let params: HoverParams = match serde_json::from_value(request.params.clone()) {
            Ok(params) => params,
            Err(error) => {
                let message = format!("invalid params: {error}");
                return self.reply_error(id, ErrorCode::InvalidParams, message);
            }
        };
        let position_params = params.text_document_position_params;
        let hover = self
            .published
            .get(&position_params.text_document.uri)
            .and_then(|published| {
                hover::hover_at(&published.file, position_params.position).or_else(|| {
                    hover::hover_at(&published.duplicates, position_params.position)
                })
            });
        let result = hover
            .map(|hover| serde_json::to_value(hover).unwrap_or(Value::Null))
            .unwrap_or(Value::Null);
        self.reply_ok(id, result);
    }

    /// Publishes both groups in one notification and remembers them for the next edit.
    fn publish(&mut self, uri: &Uri, published: Published, version: Option<i32>) {
        self.send(uri, &published, version);
        self.published.insert(uri.clone(), published);
    }

    fn send(&self, uri: &Uri, published: &Published, version: Option<i32>) {
        messages::publish(&self.sink, uri, published.combined(), version);
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
        DidChangeConfigurationParams, DidChangeTextDocumentParams, DidChangeWatchedFilesParams,
        DidCloseTextDocumentParams, DidOpenTextDocumentParams, DidSaveTextDocumentParams,
        FileChangeType, FileEvent, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, VersionedTextDocumentIdentifier,
        notification::PublishDiagnostics,
    };
    use serde_json::{Value, json};

    fn document_uri() -> Uri {
        "file:///tmp/fixture/a.ts".parse().expect("valid uri")
    }

    fn server() -> (Server, Arc<Mutex<Vec<Message>>>) {
        server_with(Settings::default(), Duration::from_millis(10))
    }

    fn server_with(
        settings: Settings,
        interval: Duration,
    ) -> (Server, Arc<Mutex<Vec<Message>>>) {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink_messages = Arc::clone(&messages);
        let sink: MessageSink = Arc::new(move |message| {
            sink_messages.lock().expect("lock messages").push(message);
        });
        let server = Server::with_debounce_interval(
            sink,
            vec![PathBuf::from("/tmp/fixture")],
            settings,
            interval,
        );
        (server, messages)
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
        open_text(server, "console.log(x);\n");
    }

    fn open_text(server: &mut Server, text: &str) {
        server.did_open(DidOpenTextDocumentParams {
            text_document: TextDocumentItem {
                uri: document_uri(),
                language_id: "typescript".to_owned(),
                version: 1,
                text: text.to_owned(),
            },
        });
    }

    /// Sends `textDocument/hover` and returns its `result` value.
    fn hover(
        server: &mut Server,
        messages: &Arc<Mutex<Vec<Message>>>,
        id: i32,
        line: u32,
        character: u32,
    ) -> Value {
        let params = json!({
            "textDocument": { "uri": document_uri() },
            "position": { "line": line, "character": character }
        });
        assert!(server.handle_request(&request(id, "textDocument/hover", params)));
        response_for(messages, id).expect("hover response")
    }

    fn change(server: &mut Server, version: i32, text: &str) {
        server.did_change(DidChangeTextDocumentParams {
            text_document: VersionedTextDocumentIdentifier {
                uri: document_uri(),
                version,
            },
            content_changes: vec![TextDocumentContentChangeEvent {
                range: None,
                range_length: None,
                text: text.to_owned(),
            }],
        });
    }

    fn configure(server: &mut Server, settings: Value) {
        let notification = Notification::new(
            DidChangeConfiguration::METHOD.to_owned(),
            DidChangeConfigurationParams { settings },
        );
        server.handle_notification(&notification);
    }

    fn watched(server: &mut Server, uris: &[&str]) {
        let changes = uris
            .iter()
            .map(|uri| {
                let uri = uri.parse().expect("valid uri");
                FileEvent::new(uri, FileChangeType::CHANGED)
            })
            .collect();
        let notification = Notification::new(
            DidChangeWatchedFiles::METHOD.to_owned(),
            DidChangeWatchedFilesParams { changes },
        );
        server.handle_notification(&notification);
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

        assert!(!server.handle_request(&request(9, "textDocument/codeAction", json!({}))));
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

    #[test]
    fn analyze_on_open_false_keeps_the_document_unpublished() {
        let (mut server, messages) = server_with(
            Settings {
                analyze_on_open: false,
                ..Settings::default()
            },
            Duration::from_millis(10),
        );

        open(&mut server);

        assert!(published(&messages).is_empty());
        assert!(server.documents.contains_key(&document_uri()));
    }

    #[test]
    fn did_change_configuration_republishes_open_documents_when_enabled() {
        let (mut server, messages) = server();
        open(&mut server);
        assert_eq!(published(&messages).len(), 1);

        configure(&mut server, json!({ "nullscan": { "analyzeOnOpen": false } }));
        assert!(!server.settings.analyze_on_open);
        assert_eq!(published(&messages).len(), 1);

        configure(&mut server, json!({ "nullscan": { "analyzeOnOpen": true } }));
        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert_eq!(publishes[1].uri, document_uri());
        assert_eq!(publishes[1].version, Some(1));
        assert!(!publishes[1].diagnostics.is_empty());
    }

    #[test]
    fn analyze_on_save_false_keeps_the_save_silent() {
        let (mut server, messages) = server_with(
            Settings {
                analyze_on_save: false,
                ..Settings::default()
            },
            Duration::from_millis(10),
        );
        open(&mut server);
        change(&mut server, 2, "const value = 1;\n");

        server.did_save(DidSaveTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: document_uri() },
            text: None,
        });

        assert_eq!(published(&messages).len(), 1);
    }

    #[test]
    fn analyze_on_type_schedules_the_latest_buffer() {
        let (mut server, messages) = server_with(
            Settings {
                analyze_on_type: true,
                ..Settings::default()
            },
            Duration::from_millis(10),
        );
        open(&mut server);
        change(&mut server, 2, "const value = 1;\n");
        change(&mut server, 3, "console.log(value);\n");

        let fired = server
            .debounce_results()
            .recv_timeout(Duration::from_secs(2))
            .expect("debounced uri");
        assert_eq!(fired, document_uri());
        assert_eq!(published(&messages).len(), 1);

        server.analyze_debounced(fired);

        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert_eq!(publishes[1].version, Some(3));
        assert!(!publishes[1].diagnostics.is_empty());
    }

    #[test]
    fn analyze_on_type_disabled_schedules_nothing() {
        let (mut server, _messages) = server();
        open(&mut server);

        change(&mut server, 2, "console.log(value);\n");

        assert!(
            server
                .debounce_results()
                .recv_timeout(Duration::from_millis(50))
                .is_err()
        );
    }

    #[test]
    fn a_closed_document_drops_its_pending_debounced_analysis() {
        let (mut server, messages) = server_with(
            Settings {
                analyze_on_type: true,
                ..Settings::default()
            },
            Duration::from_millis(10),
        );
        open(&mut server);
        change(&mut server, 2, "console.log(value);\n");
        server.did_close(DidCloseTextDocumentParams {
            text_document: TextDocumentIdentifier { uri: document_uri() },
        });
        let publishes_after_close = published(&messages).len();

        let fired = server
            .debounce_results()
            .recv_timeout(Duration::from_secs(2))
            .expect("debounced uri");
        server.analyze_debounced(fired);

        assert_eq!(published(&messages).len(), publishes_after_close);
    }

    #[test]
    fn a_config_file_change_republishes_open_documents() {
        let (mut server, messages) = server();
        open(&mut server);
        assert_eq!(published(&messages).len(), 1);

        watched(&mut server, &["file:///tmp/fixture/src/b.ts"]);
        assert_eq!(published(&messages).len(), 1);

        watched(&mut server, &["file:///tmp/fixture/nullscan.toml"]);
        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert_eq!(publishes[1].uri, document_uri());
    }

    #[test]
    fn hover_describes_the_rule_under_the_cursor() {
        let (mut server, messages) = server();
        open(&mut server);

        let result = hover(&mut server, &messages, 20, 0, 3);

        assert_eq!(result["contents"]["kind"], "markdown");
        let value = result["contents"]["value"].as_str().expect("markdown value");
        assert!(value.contains("**no-console**"), "hover: {value}");
        assert!(value.contains("Unexpected console call"), "hover: {value}");
        assert!(value.contains("unexpected console call"), "hover: {value}");
        assert_eq!(
            result["range"],
            json!({
                "start": { "line": 0, "character": 0 },
                "end": { "line": 0, "character": 14 }
            })
        );
    }

    #[test]
    fn hover_over_whitespace_returns_null() {
        let (mut server, messages) = server();
        open(&mut server);

        let result = hover(&mut server, &messages, 21, 0, 14);

        assert!(result.is_null(), "hover: {result}");
    }

    #[test]
    fn hover_in_a_multi_rule_file_picks_the_diagnostic_under_the_cursor() {
        let (mut server, messages) = server();
        open_text(&mut server, "console.log(x);\nconst y: any = 1;\n");

        let console = hover(&mut server, &messages, 22, 0, 0);
        let console_value = console["contents"]["value"].as_str().expect("hover value");
        assert!(console_value.contains("**no-console**"), "hover: {console_value}");

        let any = hover(&mut server, &messages, 23, 1, 10);
        let any_value = any["contents"]["value"].as_str().expect("hover value");
        assert!(
            any_value.contains("**no-explicit-any**"),
            "hover: {any_value}"
        );
        assert!(
            any_value.contains("Avoid the explicit 'any' type"),
            "hover: {any_value}"
        );
    }

    fn report_with_duplicate() -> nullscan_workspace::ScanReport {
        use nullscan_core::{Duplicate, DuplicateLocation, Severity};
        use nullscan_workspace::{FileResult, ScanReport};

        ScanReport {
            files: vec![
                FileResult {
                    file: PathBuf::from("/tmp/fixture/a.ts"),
                    language: Language::TypeScript,
                    source: "one\ntwo\nthree\nfour\n".to_owned(),
                    diagnostics: vec![nullscan_core::Diagnostic {
                        rule: "no-console",
                        severity: Severity::Info,
                        line: 1,
                        column: 1,
                        end_line: 1,
                        end_column: 2,
                        message: "unexpected console call".to_owned(),
                    }],
                },
                FileResult {
                    file: PathBuf::from("/tmp/fixture/b.ts"),
                    language: Language::TypeScript,
                    source: "alpha\nbeta\ngamma\ndelta\n".to_owned(),
                    diagnostics: Vec::new(),
                },
            ],
            duplicates: vec![Duplicate {
                token_count: 35,
                a: DuplicateLocation {
                    file: PathBuf::from("/tmp/fixture/a.ts"),
                    start_line: 2,
                    end_line: 4,
                },
                b: DuplicateLocation {
                    file: PathBuf::from("/tmp/fixture/b.ts"),
                    start_line: 1,
                    end_line: 3,
                },
            }],
            warnings: Vec::new(),
            duration: Duration::ZERO,
            truncated: false,
            cancelled: false,
            config_path: None,
            disabled_rules: Vec::new(),
        }
    }

    #[test]
    fn a_scan_publishes_duplicates_on_both_sides() {
        let (mut server, messages) = server();

        server.publish_report(&report_with_duplicate());

        let publishes = published(&messages);
        assert_eq!(publishes.len(), 2);
        assert_eq!(
            publishes[0].uri,
            "file:///tmp/fixture/a.ts".parse().expect("uri")
        );
        assert_eq!(
            publishes[1].uri,
            "file:///tmp/fixture/b.ts".parse().expect("uri")
        );
        for publish in &publishes {
            let duplicate = publish
                .diagnostics
                .iter()
                .find(|diagnostic| {
                    diagnostic.code
                        == Some(NumberOrString::String("duplicate-code".to_owned()))
                })
                .expect("duplicate-code diagnostic");
            assert!(duplicate.related_information.is_some());
        }
        assert_eq!(publishes[0].diagnostics.len(), 2);
        assert_eq!(publishes[1].diagnostics.len(), 1);
    }

    #[test]
    fn dropping_duplicates_keeps_the_file_group() {
        let (mut server, messages) = server();
        server.publish_report(&report_with_duplicate());
        let uri: Uri = "file:///tmp/fixture/a.ts".parse().expect("uri");

        server.drop_duplicate_diagnostics(&uri);

        let last = published(&messages).pop().expect("republish after edit");
        assert_eq!(last.uri, uri);
        assert_eq!(last.version, None);
        let codes: Vec<_> = last
            .diagnostics
            .iter()
            .filter_map(|diagnostic| diagnostic.code.clone())
            .collect();
        assert_eq!(
            codes,
            vec![NumberOrString::String("no-console".to_owned())],
            "only the file group survives"
        );
        assert!(server.published[&uri].duplicates.is_empty());
    }
}
