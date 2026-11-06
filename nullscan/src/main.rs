mod sarif;

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use clap::{Parser, ValueEnum};
use nullscan_workspace::{ScanOptions, ScanReport, config, render_json, scan, walk};

#[derive(Clone, ValueEnum)]
enum Format {
    /// Human-readable, one diagnostic per line.
    Text,
    /// SARIF 2.1.0 JSON, for GitHub Code Scanning / editor SARIF viewers.
    Sarif,
    /// Machine-readable JSON contract for editors and CI.
    Json,
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

    /// Stop after this many diagnostics (deeper findings are dropped)
    #[arg(long)]
    max_diagnostics: Option<usize>,
}

impl std::fmt::Display for Format {
    fn fmt(&self, f: &mut std::fmt::Formatter) -> std::fmt::Result {
        f.write_str(match self {
            Format::Text => "text",
            Format::Sarif => "sarif",
            Format::Json => "json",
        })
    }
}

fn main() -> ExitCode {
    let args = Args::parse();

    let loaded = match config::load(args.config.as_deref().map(Path::new)) {
        Ok(loaded) => loaded,
        Err(error) => {
            eprintln!("error: {error}");
            return ExitCode::from(2);
        }
    };

    let paths: Vec<PathBuf> = args.paths.iter().map(PathBuf::from).collect();
    if !walk::any_existing_path(&paths) {
        eprintln!("error: no existing input path");
        return ExitCode::from(2);
    }

    let mut report = scan(ScanOptions {
        paths: &paths,
        config: loaded.config,
        config_path: loaded.path,
        max_diagnostics: args.max_diagnostics,
    });
    report.warnings.extend(loaded.warnings);
    report.warnings.sort();
    report.warnings.dedup();

    match args.format {
        Format::Text => {
            print_warnings(&report.warnings);
            print_text(&report);
        }
        Format::Sarif => {
            print_warnings(&report.warnings);
            println!("{}", sarif::render(&report.files, &report.duplicates));
        }
        Format::Json => print!("{}", render_json(&report)),
    }

    if has_findings(&report) {
        ExitCode::from(1)
    } else {
        ExitCode::SUCCESS
    }
}

fn print_warnings(warnings: &[String]) {
    for warning in warnings {
        eprintln!("{warning}");
    }
}

fn print_text(report: &ScanReport) {
    for file in &report.files {
        for diagnostic in &file.diagnostics {
            println!(
                "{}:{}:{}: [{}] {}",
                file.file.display(),
                diagnostic.line,
                diagnostic.column,
                diagnostic.rule,
                diagnostic.message
            );
        }
    }
    for duplicate in &report.duplicates {
        println!(
            "duplicate code ({} tokens): {}:{}-{} <-> {}:{}-{}",
            duplicate.token_count,
            duplicate.a.file.display(),
            duplicate.a.start_line,
            duplicate.a.end_line,
            duplicate.b.file.display(),
            duplicate.b.start_line,
            duplicate.b.end_line,
        );
    }
}

fn has_findings(report: &ScanReport) -> bool {
    report.truncated
        || !report.duplicates.is_empty()
        || report
            .files
            .iter()
            .any(|file| !file.diagnostics.is_empty())
}
