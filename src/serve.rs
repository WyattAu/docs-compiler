//! The dev server's reusable half (Wave 2): compiling a directory of
//! markdown into a directory of HTML, URL routing with a traversal
//! guard, and MIME mapping. The socket loop and the file watcher are
//! thin wrappers in the binary; everything with logic lives here and is
//! unit-tested.
//!
//! Since Wave 2c every page compiles to a FULL HTML document carrying
//! the estate-seo floor's head artifacts (title, description, canonical,
//! Open Graph, JSON-LD); a `site.toml` with `base_url` additionally
//! emits `llms.txt`, `sitemap.xml`, and `robots.txt` (with the
//! AI-crawler policy as a recorded decision). Per-page inputs come from
//! `---` front-matter (`title`, `description`, `slug`, `date`).
//!
//! Design note (plan ledger W2-3): the server lives in docs-compiler,
//! not docs-core - docs-core is L2 with no web framework, and serving
//! is a CLI concern. The server never dies on a bad page: compile
//! errors are collected per page, printed, and the site keeps serving
//! the last good HTML (fail-loud, not fail-dead).

use crate::compress::{self, Tools};
use crate::seo::{self, SeoPage, SiteConfig};
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
    /// Content-quality warnings (title > 60 chars, description > 155,
    /// missing description): the estate-seo floor's guidance, not errors.
    pub warnings: Vec<String>,
    /// The SEO view of every built page (route, title, description,
    /// date) - also the source for llms.txt and sitemap.xml.
    pub pages: Vec<SeoPage>,
    /// Whether a `site.toml` provided site-wide settings.
    pub configured: bool,
    /// Artifacts written (pages, llms.txt, sitemap, robots) - the
    /// precompression pass runs over exactly these.
    pub artifacts: Vec<PathBuf>,
    /// Compressed sidecars produced (`.gz`/`.br`/`.zst`/`.dcz` files).
    pub compressed: usize,
    /// Whether a shared zstd dictionary was trained and emitted
    /// (`site.dict`, served with `Use-As-Dictionary`).
    pub dictionary: bool,
}

impl SiteReport {
    /// Whether every page compiled.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// Compile every `.md` under `src` (recursive, hidden dirs skipped) into
/// the mirrored `.html` layout under `out`, as full documents with the
/// SEO head. Front-matter (`title`, `description`, `slug`, `date`)
/// overrides the title/description/route/date fallbacks. Directive roots
/// are each page's own directory, matching the single-file CLI's
/// default. One bad page never blocks the others: its error lands in
/// the report. A `site.toml` with `base_url` additionally emits
/// `llms.txt`, `sitemap.xml`, and `robots.txt` into `out`.
///
/// # Errors
/// I/O failures on the tree walk or output creation.
pub fn compile_site(src: &Path, out: &Path) -> io::Result<SiteReport> {
    let config = SiteConfig::load(src);
    let mut report = SiteReport {
        configured: config.is_some(),
        ..SiteReport::default()
    };
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
                build_one_page(src, out, &path, config.as_ref(), &mut report);
            }
        }
    }
    if let Some(config) = &config {
        let page_refs: &[SeoPage] = &report.pages;
        for (name, content) in [
            ("llms.txt", seo::llms_txt(config, page_refs)),
            ("sitemap.xml", seo::sitemap_xml(config, page_refs)),
            ("robots.txt", seo::robots_txt(config)),
        ] {
            let path = out.join(name);
            std::fs::write(&path, content)?;
            report.artifacts.push(path);
        }
    }
    precompress(out, &mut report);
    Ok(report)
}

/// The precompression pass (Wave 2c.5): sidecars for every artifact,
/// plus the shared zstd dictionary and `.dcz` page encodings when the
/// site has enough pages to train on. Absent tools degrade gracefully
/// (warning, not failure) - the estate's spawn-don't-link doctrine.
fn precompress(out: &Path, report: &mut SiteReport) {
    let tools = Tools::detect();
    if !tools.any() {
        report
            .warnings
            .push("no gzip/brotli/zstd on PATH: precompression skipped".into());
        return;
    }
    let mut compressed = 0usize;
    let artifacts = report.artifacts.clone();
    for artifact in &artifacts {
        match compress::sidecars(&tools, artifact) {
            Ok(encodings) => compressed += encodings.len(),
            Err(e) => report
                .warnings
                .push(format!("{}: compression failed: {e}", artifact.display())),
        }
    }
    let dict_path = out.join("site.dict");
    match compress::train_dictionary(&tools, &artifacts, &dict_path) {
        Ok(true) => {
            report.dictionary = true;
            compressed += 1; // the dictionary itself is an artifact
            for artifact in &artifacts {
                if matches!(
                    artifact.extension().and_then(|e| e.to_str()),
                    Some("html") | Some("txt") | Some("xml")
                ) && compress::dcz_sidecar(&tools, &dict_path, artifact).unwrap_or(false)
                {
                    compressed += 1;
                }
            }
        }
        Ok(false) => {
            report.warnings.push(format!(
                "shared dictionary skipped ({} artifacts: zstd trains on >= 8 samples)",
                artifacts.len()
            ));
        }
        Err(e) => report
            .warnings
            .push(format!("dictionary training failed: {e}")),
    }
    report.compressed = compressed;
}

/// Build one page: front-matter, compile, head, write, report.
fn build_one_page(
    src: &Path,
    out: &Path,
    path: &Path,
    config: Option<&SiteConfig>,
    report: &mut SiteReport,
) {
    let rel = match path.strip_prefix(src) {
        Ok(r) => r.to_path_buf(),
        Err(_) => path.to_path_buf(),
    };
    let source = match std::fs::read_to_string(path) {
        Ok(s) => s,
        Err(e) => {
            report.errors.push((path.to_path_buf(), e.to_string()));
            return;
        }
    };
    let (meta, body_markdown) = seo::split_front_matter(&source);
    let root = path
        .parent()
        .filter(|p| !p.as_os_str().is_empty())
        .map_or_else(|| PathBuf::from("."), Path::to_path_buf);
    let body = match compile(&body_markdown, &BuildContext::new(root)) {
        Ok(html) => html,
        Err(e) => {
            report.errors.push((path.to_path_buf(), e.to_string()));
            return;
        }
    };

    let stem = meta.slug.clone().unwrap_or_else(|| {
        rel.file_stem()
            .unwrap_or_default()
            .to_string_lossy()
            .into_owned()
    });
    let dir = rel.parent().unwrap_or(Path::new(""));
    let route = if dir.as_os_str().is_empty() {
        format!("{stem}.html")
    } else {
        format!("{}/{}.html", dir.display(), stem)
    };
    let title = meta
        .title
        .clone()
        .or_else(|| seo::first_h1(&body))
        .unwrap_or_else(|| {
            rel.file_stem()
                .map(|s| s.to_string_lossy().into_owned())
                .unwrap_or_else(|| "Untitled".to_string())
        });
    let description = meta
        .description
        .clone()
        .or_else(|| seo::first_paragraph(&body))
        .unwrap_or_default();

    if title.chars().count() > 60 {
        report.warnings.push(format!(
            "{route}: title is {} chars (guidance: <= 60)",
            title.chars().count()
        ));
    }
    if description.is_empty() {
        report.warnings.push(format!(
            "{route}: no description (front-matter or first paragraph)"
        ));
    } else if description.chars().count() > 155 {
        report.warnings.push(format!(
            "{route}: description is {} chars (guidance: <= 155)",
            description.chars().count()
        ));
    }

    let document = seo::document(&body, &meta, &title, &description, &route, config);
    let target = out.join(&route);
    if let Some(parent) = target.parent() {
        if let Err(e) = std::fs::create_dir_all(parent) {
            report.errors.push((path.to_path_buf(), e.to_string()));
            return;
        }
    }
    if let Err(e) = std::fs::write(&target, document) {
        report.errors.push((path.to_path_buf(), e.to_string()));
        return;
    }
    report.built.push(path.to_path_buf());
    report.artifacts.push(target);
    report.pages.push(SeoPage {
        route,
        title,
        description,
        date: meta.date,
    });
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
        "txt" => "text/plain; charset=utf-8",
        "woff2" => "font/woff2",
        _ => "text/plain; charset=utf-8",
    }
}
