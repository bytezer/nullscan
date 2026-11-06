//! `nullscan-lsp`: the stdio language server for the editor integration.
//!
//! stdout is reserved for the JSON-RPC channel; every log line goes to stderr. Argument
//! parsing is hand-written so this binary never pulls in the CLI's `clap` stack.

mod convert;
mod server;
mod uri;

use std::env;
use std::process::ExitCode;

use lsp_server::{Connection, ErrorCode, Message, Request, Response};
use lsp_types::{
    InitializeParams, InitializeResult, PositionEncodingKind, SaveOptions, ServerCapabilities,
    ServerInfo, TextDocumentSyncCapability, TextDocumentSyncKind, TextDocumentSyncOptions,
    TextDocumentSyncSaveOptions,
};

use server::Server;

const USAGE: &str = "usage: nullscan-lsp --stdio\n       nullscan-lsp --version";

enum Command {
    Stdio,
    Version,
}

fn main() -> ExitCode {
    match parse_args(env::args().skip(1)) {
        Ok(Command::Version) => {
            println!("nullscan-lsp {}", env!("CARGO_PKG_VERSION"));
            ExitCode::SUCCESS
        }
        Ok(Command::Stdio) => run_stdio(),
        Err(message) => {
            eprintln!("error: {message}");
            eprintln!("{USAGE}");
            ExitCode::from(2)
        }
    }
}

fn parse_args(args: impl Iterator<Item = String>) -> Result<Command, String> {
    let args: Vec<String> = args.collect();
    match args.as_slice() {
        [flag] if flag == "--stdio" => Ok(Command::Stdio),
        [flag] if flag == "--version" => Ok(Command::Version),
        [] => Err("missing required flag --stdio".to_owned()),
        [unknown, ..] => Err(format!("unknown argument '{unknown}'")),
    }
}

fn run_stdio() -> ExitCode {
    init_logger();

    let (connection, io_threads) = Connection::stdio();
    let result = serve(&connection);
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
fn serve(connection: &Connection) -> Result<(), String> {
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
    );
    for message in &connection.receiver {
        match message {
            Message::Request(request) => {
                if connection.handle_shutdown(&request).map_err(stringify)? {
                    break;
                }
                reject_request(connection, &request);
            }
            Message::Notification(notification) => server.handle_notification(&notification),
            Message::Response(_) => log::debug!("ignoring unexpected response"),
        }
    }
    Ok(())
}

fn message_sink(connection: &Connection) -> server::MessageSink {
    let sender = connection.sender.clone();
    Box::new(move |message| {
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
