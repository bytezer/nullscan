//! `nullscan-lsp`: the stdio language server for the editor integration.
//!
//! stdout is reserved for the JSON-RPC channel; every log line goes to stderr. Argument
//! parsing is hand-written so this binary never pulls in the CLI's `clap` stack.

mod code_action;
mod config_edit;
mod convert;
mod debounce;
mod hover;
mod messages;
mod requests;
mod rules;
mod server;
mod settings;
mod uri;

use std::env;
use std::path::PathBuf;
use std::process::ExitCode;
use std::sync::Arc;

use lsp_server::{Connection, ErrorCode, Message, Request, Response};
use lsp_types::{
    CodeActionKind, CodeActionOptions, CodeActionProviderCapability, HoverProviderCapability,
    InitializeParams, InitializeResult, OneOf, PositionEncodingKind, SaveOptions,
    ServerCapabilities, ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind,
    TextDocumentSyncOptions, TextDocumentSyncSaveOptions, WorkspaceFoldersServerCapabilities,
    WorkspaceServerCapabilities,
};

use messages::MessageSink;
use server::Server;
use settings::Settings;

const USAGE: &str =
    "usage: nullscan-lsp --stdio [--config <path>]\n       nullscan-lsp --version";

#[derive(Debug, PartialEq, Eq)]
enum Command {
    /// Serve stdio, optionally with one explicit `nullscan.toml` for every document.
    Stdio { config: Option<PathBuf> },
    Version,
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)) {
        Ok(Command::Version) => {
            println!("nullscan-lsp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Stdio { config }) => run_stdio(config),
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

/// Parses the hand-written flags; order does not matter and `--config=<path>` is the
/// same as `--config <path>`.
fn parse_args(mut args: impl Iterator<Item = String>) -> Result<Command, String> {
    let mut stdio = false;
    let mut version = false;
    let mut config = None;

    while let Some(arg) = args.next() {
        match arg.as_str() {
            "--stdio" => stdio = true,
            "--version" => version = true,
            "--config" => {
                let value = args
                    .next()
                    .ok_or_else(|| "missing value for --config".to_owned())?;
                config = Some(PathBuf::from(value));
            }
            other => {
                let Some(value) = other.strip_prefix("--config=") else {
                    return Err(format!("unknown argument '{other}'"));
                };
                if value.is_empty() {
                    return Err("missing value for --config".to_owned());
                }
                config = Some(PathBuf::from(value));
            }
        }
    }

    if version {
        return Ok(Command::Version);
    }
    if !stdio {
        return Err("missing required flag --stdio".to_owned());
    }
    Ok(Command::Stdio { config })
}

fn run_stdio(config: Option<PathBuf>) -> ExitCode {
    init_logger();

    let (connection, io_threads) = Connection::stdio();
    let result = serve(&connection, config);
    // Dropping the connection closes the writer channel before joining the I/O threads.
    drop(connection);

    if let Err(error) = &result {
        log::error!("{error}");
        eprintln!("error: {error}");
    }
    if let Err(error) = io_threads.join() {
        log::error!("I/O thread failed: {error}");
    }

    match result {
        Ok(()) => ExitCode::SUCCESS,
        Err(_) => ExitCode::from(1),
    }
}

/// Runs the initialize handshake and then the message dispatch loop.
///
/// The loop waits on client messages and on finished workspace scans at the same time, so
/// `$/cancelRequest` and edits are handled while a scan is still running.
fn serve(connection: &Connection, config: Option<PathBuf>) -> Result<(), String> {
    let (id, params) = connection.initialize_start().map_err(stringify)?;
    let init_params: InitializeParams = serde_json::from_value(params)
        .map_err(|error| format!("invalid initialize params: {error}"))?;

    let result = InitializeResult {
        capabilities: server_capabilities(),
        server_info: Some(ServerInfo {
            name: "nullscan-lsp".to_owned(),
            version: Some(env!("CARGO_PKG_VERSION").to_owned()),
        }),
    };
    connection
        .initialize_finish(
            id,
            serde_json::to_value(result).map_err(|error| error.to_string())?,
        )
        .map_err(stringify)?;

    let mut server = Server::new(
        message_sink(connection),
        server::workspace_folders(&init_params),
        initial_settings(&init_params),
        config,
    );
    let job_results = server.job_results();
    let debounce_results = server.debounce_results();

    loop {
        crossbeam_channel::select! {
            recv(connection.receiver) -> message => {
                let message = message.map_err(|error| format!("transport closed: {error}"))?;
                if !dispatch(connection, &mut server, message)? {
                    break;
                }
            }
            recv(job_results) -> job => {
                let job = job.map_err(|error| format!("job channel closed: {error}"))?;
                server.finish_job(job);
            }
            recv(debounce_results) -> uri => {
                let uri = uri.map_err(|error| format!("debounce channel closed: {error}"))?;
                server.analyze_debounced(uri);
            }
        }
    }
    Ok(())
}

/// Settings the client sent with `initialize`; a missing or `null` payload means defaults.
fn initial_settings(params: &InitializeParams) -> Settings {
    params
        .initialization_options
        .as_ref()
        .map_or_else(Settings::default, Settings::from_value)
}

/// Routes one incoming message; returns `false` when the client asked to shut down.
fn dispatch(
    connection: &Connection,
    server: &mut Server,
    message: Message,
) -> Result<bool, String> {
    match message {
        Message::Request(request) => {
            if connection.handle_shutdown(&request).map_err(stringify)? {
                return Ok(false);
            }
            if !server.handle_request(&request) {
                reject_request(connection, &request);
            }
        }
        Message::Notification(notification) => server.handle_notification(&notification),
        Message::Response(_) => log::debug!("ignoring unexpected response"),
    }
    Ok(true)
}

fn message_sink(connection: &Connection) -> MessageSink {
    let sender = connection.sender.clone();
    Arc::new(move |message| {
        if sender.send(message).is_err() {
            log::debug!("dropping outgoing message: channel closed");
        }
    })
}

fn server_capabilities() -> ServerCapabilities {
    ServerCapabilities {
        text_document_sync: Some(TextDocumentSyncCapability::Options(
            TextDocumentSyncOptions {
                open_close: Some(true),
                change: Some(TextDocumentSyncKind::FULL),
                save: Some(TextDocumentSyncSaveOptions::SaveOptions(SaveOptions {
                    include_text: Some(false),
                })),
                ..TextDocumentSyncOptions::default()
            },
        )),
        position_encoding: Some(PositionEncodingKind::UTF16),
        hover_provider: Some(HoverProviderCapability::Simple(true)),
        code_action_provider: Some(CodeActionProviderCapability::Options(CodeActionOptions {
            code_action_kinds: Some(vec![CodeActionKind::QUICKFIX]),
            resolve_provider: Some(false),
            ..CodeActionOptions::default()
        })),
        // Multi-root workspaces: accept the initial folders and later
        // `workspace/didChangeWorkspaceFolders` notifications.
        workspace: Some(WorkspaceServerCapabilities {
            workspace_folders: Some(WorkspaceFoldersServerCapabilities {
                supported: Some(true),
                change_notifications: Some(OneOf::Left(true)),
            }),
            file_operations: None,
        }),
        ..ServerCapabilities::default()
    }
}

fn reject_request(connection: &Connection, request: &Request) {
    log::warn!("unsupported request: {}", request.method);
    let response = Response::new_err(
        request.id.clone(),
        ErrorCode::MethodNotFound as i32,
        format!("unsupported request: {}", request.method),
    );
    let _ = connection.sender.send(response.into());
}

fn init_logger() {
    env_logger::Builder::from_env(env_logger::Env::default().default_filter_or("info"))
        .target(env_logger::Target::Stderr)
        .init();
}

fn stringify(error: impl std::fmt::Display) -> String {
    error.to_string()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn parse(args: &[&str]) -> Result<Command, String> {
        parse_args(args.iter().map(|arg| (*arg).to_owned()))
    }

    #[test]
    fn stdio_alone_has_no_explicit_config() {
        assert_eq!(parse(&["--stdio"]), Ok(Command::Stdio { config: None }));
    }

    #[test]
    fn config_accepts_the_space_form() {
        assert_eq!(
            parse(&["--stdio", "--config", "/tmp/custom.toml"]),
            Ok(Command::Stdio {
                config: Some(PathBuf::from("/tmp/custom.toml"))
            })
        );
    }

    #[test]
    fn config_accepts_the_equals_form() {
        assert_eq!(
            parse(&["--config=/tmp/custom.toml", "--stdio"]),
            Ok(Command::Stdio {
                config: Some(PathBuf::from("/tmp/custom.toml"))
            })
        );
    }

    #[test]
    fn config_without_a_value_is_an_error() {
        assert_eq!(
            parse(&["--stdio", "--config"]),
            Err("missing value for --config".to_owned())
        );
        assert_eq!(
            parse(&["--stdio", "--config="]),
            Err("missing value for --config".to_owned())
        );
    }

    #[test]
    fn unknown_flags_are_an_error() {
        assert_eq!(
            parse(&["--stdio", "--verbose"]),
            Err("unknown argument '--verbose'".to_owned())
        );
    }

    #[test]
    fn stdio_is_required_unless_version_is_asked_for() {
        assert_eq!(
            parse(&["--config", "/tmp/custom.toml"]),
            Err("missing required flag --stdio".to_owned())
        );
        assert_eq!(parse(&[]), Err("missing required flag --stdio".to_owned()));
        assert_eq!(parse(&["--version"]), Ok(Command::Version));
    }
}
