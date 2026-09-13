use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::Parser;

#[derive(Parser)]
#[command(name = "nullscan", about = "Fast, open-source static analysis for TypeScript")]
struct Args {
    /// TypeScript files or directories to analyze (globs are expanded by your shell)
    #[arg(required = true)]
    paths: Vec<String>,
}

fn main() -> ExitCode {
    let args = Args::parse();

    let mut files = Vec::new();
    for path in &args.paths {
        collect_ts_files(Path::new(path), &mut files);
    }
    files.sort();
    files.dedup();

    let mut any_diagnostics = false;
    let mut sources = Vec::new();
    for file in &files {
        let source = match fs::read_to_string(file) {
            Ok(s) => s,
            Err(e) => {
                eprintln!("error: could not read {}: {e}", file.display());
                continue;
            }
        };

        let diagnostics = nullscan_core::analyze(&source);
        for d in &diagnostics {
            println!("{}:{}:{}: [{}] {}", file.display(), d.line, d.column, d.rule, d.message);
        }
        any_diagnostics |= !diagnostics.is_empty();
        sources.push((file.clone(), source));
    }

    let duplicates = nullscan_core::find_duplicates(&sources);
    for dup in &duplicates {
        println!(
            "duplicate code ({} tokens): {}:{}-{} <-> {}:{}-{}",
            dup.token_count,
            dup.a.file.display(),
            dup.a.start_line,
            dup.a.end_line,
            dup.b.file.display(),
            dup.b.start_line,
            dup.b.end_line,
        );
    }
    any_diagnostics |= !duplicates.is_empty();

    if any_diagnostics {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Resolves a CLI argument to `.ts`/`.tsx` files: passed through as-is if it's a file,
/// walked recursively if it's a directory (skipping `node_modules` and dotdirs).
fn collect_ts_files(path: &Path, out: &mut Vec<PathBuf>) {
    if path.is_file() {
        out.push(path.to_path_buf());
        return;
    }
    if !path.is_dir() {
        eprintln!("error: {} does not exist", path.display());
        return;
    }

    let Ok(entries) = fs::read_dir(path) else {
        eprintln!("error: could not read directory {}", path.display());
        return;
    };
    for entry in entries.flatten() {
        let entry_path = entry.path();
        let name = entry.file_name();
        let name = name.to_string_lossy();
        if name.starts_with('.') || name == "node_modules" {
            continue;
        }
        if entry_path.is_dir() {
            collect_ts_files(&entry_path, out);
        } else if matches!(entry_path.extension().and_then(|e| e.to_str()), Some("ts" | "tsx")) {
            out.push(entry_path);
        }
    }
}
