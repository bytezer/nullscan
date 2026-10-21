mod config;
mod sarif;

use std::fs;
use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use nullscan_core::Language;
use rayon::prelude::*;

#[derive(Clone, ValueEnum)]
enum Format {
    /// Human-readable, one diagnostic per line.
    Text,
    /// SARIF 2.1.0 JSON, for GitHub Code Scanning / editor SARIF viewers.
    Sarif,
}

#[derive(Parser)]
#[command(name = "nullscan", about = "Fast, open-source static analysis for TypeScript")]
struct Args {
    /// TypeScript files or directories to analyze (globs are expanded by your shell)
    #[arg(required = true)]
    paths: Vec<String>,

    #[arg(long, value_enum, default_value_t = Format::Text)]
    format: Format,

    /// Path to a nullscan.toml config file (default: ./nullscan.toml if present)
    #[arg(long)]
    config: Option<String>,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Format::Text => "text",
            Format::Sarif => "sarif",
        })
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    let config = match config::load(args.config.as_deref()) {
        Ok(config) => config,
        Err(e) => {
            eprintln!("error: {e}");
            return ExitCode::FAILURE;
        }
    };

    let mut files = Vec::new();
    for path in &args.paths {
        collect_files(Path::new(path), &mut files);
    }
    files.sort();
    files.dedup();

    // Read + analyze in parallel; `par_iter().map(...).collect()` preserves the input
    // order, so output stays deterministic even though the work doesn't run in order.
    let results: Vec<(PathBuf, Language, String, Vec<nullscan_core::Diagnostic>)> = files
        .par_iter()
        .filter_map(|(file, language)| match fs::read_to_string(file) {
            Ok(source) => {
                let diagnostics = nullscan_core::analyze(&source, *language, &config);
                Some((file.clone(), *language, source, diagnostics))
            }
            Err(e) => {
                eprintln!("error: could not read {}: {e}", file.display());
                None
            }
        })
        .collect();

    let mut any_diagnostics = false;
    let mut sources = Vec::with_capacity(results.len());
    let mut per_file_diagnostics = Vec::with_capacity(results.len());
    for (file, language, source, diagnostics) in results {
        any_diagnostics |= !diagnostics.is_empty();
        if matches!(args.format, Format::Text) {
            for d in &diagnostics {
                println!("{}:{}:{}: [{}] {}", file.display(), d.line, d.column, d.rule, d.message);
            }
        }
        per_file_diagnostics.push((file.clone(), diagnostics));
        sources.push((file, language, source));
    }

    let duplicates = if config.rule_enabled("duplicate-code") {
        nullscan_core::find_duplicates(&sources, config.duplicate_code_min_tokens)
    } else {
        Vec::new()
    };
    any_diagnostics |= !duplicates.is_empty();

    match args.format {
        Format::Text => {
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
        }
        Format::Sarif => {
            println!("{}", sarif::render(&per_file_diagnostics, &duplicates));
        }
    }

    if any_diagnostics {
        ExitCode::FAILURE
    } else {
        ExitCode::SUCCESS
    }
}

/// Directories that never contain hand-written source: skipping them keeps duplication
/// detection from comparing a file against its own compiled/generated output.
const SKIPPED_DIRS: &[&str] = &["node_modules", "dist", "build", ".next", "out"];

/// Resolves a CLI argument to supported source files, each tagged with its language:
/// a file is taken as-is (warning if its extension isn't supported), a directory is
/// walked recursively (skipping dotdirs and `SKIPPED_DIRS`).
fn collect_files(path: &Path, out: &mut Vec<(PathBuf, Language)>) {
    if path.is_file() {
        match Language::from_path(path) {
            Some(language) => out.push((path.to_path_buf(), language)),
            None => eprintln!("warning: skipping {}: unsupported file type", path.display()),
        }
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
        if name.starts_with('.') || SKIPPED_DIRS.contains(&name.as_ref()) {
            continue;
        }
        if entry_path.is_dir() {
            collect_files(&entry_path, out);
        } else if let Some(language) = Language::from_path(&entry_path) {
            out.push((entry_path, language));
        }
    }
}
