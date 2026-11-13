//! The dev server's reusable half (Wave 2): compiling a directory of
//! markdown into a directory of HTML, URL routing with a traversal
//! guard, and MIME mapping. The socket loop and the file watcher are
//! thin wrappers in the binary; everything with logic lives here and is
//! unit-tested.
//!
//! Design note (plan ledger W2-3): the server lives in docs-compiler,
//! not docs-core - docs-core is L2 with no web framework, and serving
//! is a CLI concern. The server never dies on a bad page: compile
//! errors are collected per page, printed, and the site keeps serving
//! the last good HTML (fail-loud, not fail-dead).

use crate::{compile, BuildContext};
use std::io;
use std::path::{Path, PathBuf};

/// What one site compilation pass produced.
#[derive(Debug, Default, Clone)]
pub struct SiteReport {
    /// Pages written (source paths).
    pub built: Vec<PathBuf>,
    /// Pages that failed: (source path, error text).
    pub errors: Vec<(PathBuf, String)>,
}

impl SiteReport {
    /// Whether every page compiled.
    #[must_use]
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Compile every `.md` under `src` (recursive, hidden dirs skipped) into
/// the mirrored `.html` layout under `out`. Directive roots are each
/// page's own directory, matching the single-file CLI's default. One bad
/// page never blocks the others: its error lands in the report.
///
/// # Errors
/// I/O failures on the tree walk or output creation.
pub fn compile_site(src: &Path, out: &Path) -> io::Result<SiteReport> {
    let mut report = SiteReport::default();
    let mut stack = vec![src.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            let name = entry.file_name();
            let name = name.to_string_lossy();
            if path.is_dir() {
                if !name.starts_with('.') {
                    stack.push(path);
                }
            } else if name.ends_with(".md") {
                let rel = path.strip_prefix(src).unwrap_or(&path).to_path_buf();
                let target_rel = rel.with_extension("html");
                let target = out.join(&target_rel);
                if let Some(parent) = target.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                let source = match std::fs::read_to_string(&path) {
                    Ok(s) => s,
                    Err(e) => {
                        report.errors.push((path, e.to_string()));
                        continue;
                    }
                };
                let root = path
                    .parent()
                    .filter(|p| !p.as_os_str().is_empty())
                    .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
                match compile(&source, &BuildContext::new(root)) {
                    Ok(html) => {
                        std::fs::write(&target, html)?;
                        report.built.push(path);
                    }
                    Err(e) => report.errors.push((path, e.to_string())),
                }
            }
        }
    }
    Ok(report)
}

/// Map a request path to a file under `out`: empty → `index.html`;
/// directories resolve to their `index.html`; an extensionless path
/// falls back to `<path>.html`. Percent escapes are decoded; anything
/// that escapes `out` (or games the encoding) resolves to nothing.
#[must_use]
pub fn resolve_route(out: &Path, url_path: &str) -> Option<PathBuf> {
    let path = url_path.split(['?', '#']).next().unwrap_or("");
    let decoded = percent_decode(path)?;
    let cleaned: Vec<&str> = decoded
        .split('/')
        .filter(|seg| !seg.is_empty() && *seg != ".")
        .collect();
    if cleaned.contains(&"..") {
        return None;
    }
    let mut file = out.to_path_buf();
    for seg in &cleaned {
        file.push(seg);
    }
    if cleaned.is_empty() {
        file.push("index.html");
    }
    if file.is_dir() {
        file.push("index.html");
    }
    if !file.exists() && file.extension().is_none() {
        let with_html = file.with_extension("html");
        if with_html.is_file() {
            return Some(with_html);
        }
        return None;
    }
    if file.is_file() {
        Some(file)
    } else {
        None
    }
}

/// Minimal percent-decoding for request paths; malformed escapes
/// (`%` not followed by two hex digits) resolve to nothing.
#[must_use]
fn percent_decode(path: &str) -> Option<String> {
    let mut out = Vec::with_capacity(path.len());
    let mut bytes = path.bytes();
    while let Some(b) = bytes.next() {
        let value = if b == b'%' {
            let decode = |d: u8| (d as char).to_digit(16).map(|v| v as u8);
            let hi = decode(bytes.next()?)?;
            let lo = decode(bytes.next()?)?;
            hi.checked_mul(16)?.checked_add(lo)?
        } else {
            b
        };
        out.push(value);
    }
    String::from_utf8(out).ok()
}

/// MIME type by extension; unknown extensions are plain text (the
/// browser shows them instead of downloading).
#[must_use]
pub fn mime_for(path: &Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("") {
        "html" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json",
        "wasm" => "application/wasm",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "ico" => "image/x-icon",
        "md" => "text/markdown; charset=utf-8",
        "xml" => "application/xml",
        "woff2" => "font/woff2",
        _ => "text/plain; charset=utf-8",
    }
}
