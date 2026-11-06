//! Outgoing message helpers shared by the dispatch loop, the handlers and the scan worker.
//!
//! `stdout` is the JSON-RPC channel, so nothing here writes to the process output: every
//! user-facing message goes through the sink as a notification.

use std::sync::Arc;

use lsp_server::{Message, Notification};
use lsp_types::{
    Diagnostic as LspDiagnostic, LogMessageParams, MessageType, PublishDiagnosticsParams, Uri,
    notification::{LogMessage, Notification as _, PublishDiagnostics},
};

/// Sends one outgoing message to the client. `Arc` lets a worker thread own a clone while
/// the dispatch thread keeps the original.
pub type MessageSink = Arc<dyn Fn(Message) + Send + Sync>;

/// Pushes diagnostics for one document; `version` is `None` for files read from disk.
pub fn publish(
    sink: &MessageSink,
    uri: &Uri,
    diagnostics: Vec<LspDiagnostic>,
    version: Option<i32>,
) {
    let params = PublishDiagnosticsParams {
        uri: uri.clone(),
        diagnostics,
        version,
    };
    send(sink, PublishDiagnostics::METHOD, params);
}

/// Sends a `window/logMessage` notification (the client shows it in its output channel).
pub fn log_message(sink: &MessageSink, kind: MessageType, message: &str) {
    let params = LogMessageParams {
        typ: kind,
        message: message.to_owned(),
    };
    send(sink, LogMessage::METHOD, params);
}

fn send(sink: &MessageSink, method: &str, params: impl serde::Serialize) {
    let notification = Notification::new(method.to_owned(), params);
    sink(Message::Notification(notification));
}
