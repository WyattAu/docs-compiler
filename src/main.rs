//! The CLI: compile one markdown document to one HTML page.

use clap::Parser;
use std::path::{Path, PathBuf};

#[derive(Debug, Parser)]
#[command(
    name = "docs-compiler",
    about = "Compile a markdown document with estate directive fences to static HTML",
    version
)]
struct Cli {
    /// Input markdown file.
    input: PathBuf,
    /// Output HTML file (default: the input path with a .html suffix).
    #[arg(short, long)]
    output: Option<PathBuf>,
    /// Root directory for directive-relative paths
    /// (default: the input file's directory).
    #[arg(short, long)]
    root: Option<PathBuf>,
}

fn main() {
    let cli = Cli::parse();
    let root = cli.root.clone().unwrap_or_else(|| {
        cli.input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    });
    let source = match std::fs::read_to_string(&cli.input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", cli.input.display());
            std::process::exit(2);
        }
    };
    match docs_compiler::compile(&source, &docs_compiler::BuildContext::new(root)) {
        Ok(html) => {
            let output = cli.output.unwrap_or_else(|| {
                let mut p = cli.input.clone().into_os_string();
                p.push(".html");
                PathBuf::from(p)
            });
            if let Err(e) = std::fs::write(&output, html) {
                eprintln!("cannot write {}: {e}", output.display());
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("build failed: {e}");
            std::process::exit(1);
        }
    }
}
