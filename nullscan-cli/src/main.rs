use std::fs;
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(name = "nullscan", about = "Fast, open-source static analysis for TypeScript")]
struct Args {
    /// TypeScript file to analyze
    file: String,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let source = match fs::read_to_string(&args.file) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("error: could not read {}: {e}", args.file);
            return ExitCode::FAILURE;
        }
    };

    let diagnostics = nullscan_core::analyze(&source);
    for d in &diagnostics {
        println!("{}:{}:{}: [{}] {}", args.file, d.line, d.column, d.rule, d.message);
    }

    if diagnostics.is_empty() {
        ExitCode::SUCCESS
    } else {
        ExitCode::FAILURE
    }
}
