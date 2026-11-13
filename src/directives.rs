//! Directive resolution and static HTML emission.

use std::path::PathBuf;

use docs_symbols::Language;
use docs_witness::{check, run, Assertions, WitnessConfig};

use crate::d2::diagram;
use crate::error::CompilerError;
use crate::fences::{Directive, DirectiveKind};
use crate::lean::lean_check;

/// The build context: directive file paths resolve against this root.
#[derive(Debug, Clone)]
pub struct BuildContext {
    /// Repository/docs root (fence paths are relative to it).
    pub root: PathBuf,
}

impl BuildContext {
    /// A context rooted at `root`.
    pub fn new(root: impl Into<PathBuf>) -> Self {
        Self { root: root.into() }
    }

    fn read(&self, path: &str) -> Result<Vec<u8>, CompilerError> {
        let full = self.root.join(path);
        std::fs::read(&full).map_err(|source| CompilerError::Io {
            path: path.to_string(),
            source,
        })
    }

    pub(crate) fn read_text(&self, path: &str) -> Result<String, CompilerError> {
        let bytes = self.read(path)?;
        String::from_utf8(bytes).map_err(|_| CompilerError::Io {
            path: path.to_string(),
            source: std::io::Error::new(std::io::ErrorKind::InvalidData, "file is not valid UTF-8"),
        })
    }
}

/// Resolve one directive fence to a raw HTML block.
pub fn resolve(d: &Directive, ctx: &BuildContext) -> Result<String, CompilerError> {
    match d.kind {
        DirectiveKind::IncludeSymbol => include_symbol(d, ctx),
        DirectiveKind::Witness => witness(d, ctx),
        DirectiveKind::LeanCheck => lean_check(d, ctx),
        DirectiveKind::Diagram => diagram(d, ctx),
    }
}

/// `include-symbol file=... symbol=... [lang=rust]`
fn include_symbol(d: &Directive, ctx: &BuildContext) -> Result<String, CompilerError> {
    let file = d.require("file")?;
    let symbol = d.require("symbol")?;
    let lang = match d.attr("lang").unwrap_or("rust") {
        "rust" => Language::Rust,
        other => {
            return Err(CompilerError::Attribute {
                directive: "include-symbol",
                reason: format!("unsupported language `{other}` (0.1 ships rust)"),
            })
        }
    };
    let source = ctx.read_text(file)?;
    let span = docs_symbols::extract(&source, lang, symbol).map_err(CompilerError::Symbol)?;
    Ok(format!(
        "<figure class=\"docs-symbol\" data-anchor=\"{anchor}\"><figcaption>{file} :: {symbol}</figcaption><pre><code>{code}</code></pre></figure>",
        anchor = span.anchor_id,
        code = html_escape(&span.code),
    ))
}

/// `witness wasm=... [fuel=...] [stdout_capacity=...] [assert_exit=...]
/// [assert_stdout="..."]`
fn witness(d: &Directive, ctx: &BuildContext) -> Result<String, CompilerError> {
    let wasm_path = d.require("wasm")?;
    let wasm = ctx.read(wasm_path)?;
    let cfg = WitnessConfig {
        fuel: d
            .attr("fuel")
            .and_then(|v| v.parse().ok())
            .unwrap_or(docs_witness::DEFAULT_FUEL),
        stdout_capacity: d
            .attr("stdout_capacity")
            .and_then(|v| v.parse().ok())
            .unwrap_or(docs_witness::DEFAULT_STDOUT_CAPACITY),
    };
    let w = run(&wasm, &cfg).map_err(CompilerError::Witness)?;
    let assertions = Assertions {
        exit: d.attr("assert_exit").and_then(|v| v.parse().ok()),
        stdout: d.attr("assert_stdout").map(str::to_string),
    };
    check(&w, &assertions).map_err(CompilerError::WitnessAssert)?;

    Ok(format!(
        "<div class=\"docs-witness\" data-exit=\"{exit}\" data-fuel=\"{fuel}\"><pre class=\"docs-witness-stdout\">{stdout}</pre></div>",
        exit = w.exit,
        fuel = w.fuel_consumed,
        stdout = html_escape(&String::from_utf8_lossy(&w.stdout)),
    ))
}

/// First few lines of a diagnostics blob, for error messages.
pub(crate) fn first_lines(text: &str) -> String {
    let collected: Vec<&str> = text
        .lines()
        .filter(|l| !l.trim().is_empty())
        .take(3)
        .collect();
    collected.join(" | ")
}

/// Minimal HTML text escaping: `&`, `<`, `>`; blank-line guard included
/// (an empty line inside a raw-HTML block would end the block in
/// pulldown-cmark, so interior double newlines gain a zero-width entity
/// line).
pub(crate) fn html_escape(text: &str) -> String {
    let escaped = text
        .replace('&', "&amp;")
        .replace('<', "&lt;")
        .replace('>', "&gt;");
    escaped.replace("\n\n", "\n&#10;\n")
}
