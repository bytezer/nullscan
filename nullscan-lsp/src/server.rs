//! The document store and LSP notification handlers.
//!
//! Analysis is synchronous per file for now. Handlers only mutate the store and go
//! through [`Server::publish_for_document`], so a worker thread can take over that call
//! later without changing the protocol layer.

use std::collections::HashMap;
use std::path::{Path, PathBuf};

use lsp_server::{Message, Notification};
use lsp_types::{
    Diagnostic as LspDiagnostic, DidChangeTextDocumentParams, DidCloseTextDocumentParams,
    DidOpenTextDocumentParams, DidSaveTextDocumentParams, InitializeParams, LogMessageParams,
    MessageType, PublishDiagnosticsParams, Uri,
    notification::{
        DidChangeTextDocument, DidCloseTextDocument, DidOpenTextDocument, DidSaveTextDocument,
        LogMessage, Notification as _, PublishDiagnostics,
    },
};
use nullscan_core::{Config, Language, analyze};
use nullscan_workspace::config::load_in_dir;

use crate::convert;
use crate::uri::to_file_path;

/// Sends one outgoing message to the client. A closure keeps the transport type out of
/// this module and lets a future worker own its own clone.
pub type MessageSink = Box<dyn Fn(Message) + Send + Sync>;

/// One open buffer. Full sync keeps the whole text, so no incremental state is stored.
pub struct Document {
    text: String,
    version: i32,
}

/// Owns every open document plus the workspace folders used to resolve configs.
pub struct Server {
    sink: MessageSink,
    documents: HashMap<Uri, Document>,
    folders: Vec<PathBuf>,
}

impl Server {
    pub fn new(sink: MessageSink, folders: Vec<PathBuf>) -> Self {
        Self {
            sink,
            documents: HashMap::new(),
            folders,
        }
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
            other => log::debug!("ignoring notification {other}"),
        }
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
        self.publish_for_document(&item.uri);
    }

    /// Full sync: only the last content change matters and carries the complete text.
    fn did_change(&mut self, params: DidChangeTextDocumentParams) {
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
        self.publish_for_document(&params.text_document.uri);
    }

    fn did_close(&mut self, params: DidCloseTextDocumentParams) {
        let uri = params.text_document.uri;
        self.documents.remove(&uri);
        self.publish(&uri, Vec::new(), None);
    }

    /// Analyzes the stored buffer and publishes its diagnostics.
    pub fn publish_for_document(&self, uri: &Uri) {
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
        self.publish(uri, diagnostics, Some(document.version));
    }

    fn analyze_document(&self, uri: &Uri, text: &str, language: Language) -> Vec<LspDiagnostic> {
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
                self.log_message(MessageType::WARNING, &format!("nullscan config: {error}"));
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

    fn publish(&self, uri: &Uri, diagnostics: Vec<LspDiagnostic>, version: Option<i32>) {
        let params = PublishDiagnosticsParams {
            uri: uri.clone(),
            diagnostics,
            version,
        };
        let notification = Notification::new(PublishDiagnostics::METHOD.to_owned(), params);
        (self.sink)(Message::Notification(notification));
    }

    fn log_message(&self, kind: MessageType, message: &str) {
        let params = LogMessageParams {
            typ: kind,
            message: message.to_owned(),
        };
        let notification = Notification::new(LogMessage::METHOD.to_owned(), params);
        (self.sink)(Message::Notification(notification));
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

    use lsp_types::{
        DidChangeTextDocumentParams, DidCloseTextDocumentParams, DidOpenTextDocumentParams,
        DidSaveTextDocumentParams, TextDocumentContentChangeEvent, TextDocumentIdentifier,
        TextDocumentItem, VersionedTextDocumentIdentifier,
    };

    fn document_uri() -> Uri {
        "file:///tmp/fixture/a.ts".parse().expect("valid uri")
    }

    fn server() -> (Server, Arc<Mutex<Vec<Message>>>) {
        let messages = Arc::new(Mutex::new(Vec::new()));
        let sink_messages = Arc::clone(&messages);
        let sink: MessageSink = Box::new(move |message| {
            sink_messages.lock().expect("lock messages").push(message);
        });
        (Server::new(sink, vec![PathBuf::from("/tmp/fixture")]), messages)
    }

    fn published(messages: &Arc<Mutex<Vec<Message>>>) -> Vec<PublishDiagnosticsParams> {
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
}
