//! The dev server's reusable half (Wave 2): compiling a directory of
//! markdown into a directory of HTML, URL routing with a traversal
//! guard, and MIME mapping. The socket loop and the file watcher are
//! thin wrappers in the binary; everything with logic lives here and is
//! unit-tested.
//!
//! Since Wave 2d the site builder owns the whole tree: pages become
//! full documents wrapped in the site chrome (header, sidebar nav tree
//! built from the directory structure, per-page TOC, prev/next pager,
//! theme toggle), `src/assets/**` is mirrored to `out/_assets/**`, and
//! a search corpus (`search.json`, for the solid-fx doc-search island)
//! plus a `docs-search` binary index (`search.bin`) are emitted.
//!
//! Design note (plan ledger W2-3): the server lives in docs-compiler,
//! not docs-core - docs-core is L2 with no web framework, and serving
//! is a CLI concern. The server never dies on a bad page: compile
//! errors are collected per page, printed, and the site keeps serving
//! the last good HTML (fail-loud, not fail-dead).

use crate::chrome::{self, Chrome, NavGroup};
use crate::compress::{self};
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
    /// Artifacts written (pages, search corpus/index, llms.txt,
    /// sitemap, robots) - the precompression pass runs over these.
    pub artifacts: Vec<PathBuf>,
    /// Compressed sidecars produced (`.gz`/`.br`/`.zst`/`.dcz` files).
    pub compressed: usize,
    /// Whether a shared zstd dictionary was trained and emitted
    /// (`site.dict`, served with `Use-As-Dictionary`).
    pub dictionary: bool,
    /// Asset files copied from `src/assets/**`.
    pub assets_copied: usize,
}

impl SiteReport {
    /// Whether every page compiled.
    pub fn is_clean(&self) -> bool {
        self.errors.is_empty()
    }
}

/// One rendered page, ready for chrome wrapping and writing.
struct PageBuild {
    source: PathBuf,
    route: String,
    title: String,
    description: String,
    date: Option<String>,
    meta: seo::PageMeta,
    body: String,
    /// Plain text of the body - the search corpus input.
    text: String,
}

/// Compile every `.md` under `src` (recursive, hidden dirs skipped) into
/// the mirrored `.html` layout under `out`, wrapped in the site chrome.
/// Front-matter (`title`, `description`, `slug`, `date`) overrides the
/// title/description/route/date fallbacks. Directive roots are each
/// page's own directory, matching the single-file CLI's default.
///
/// A `site.toml` with `base_url` additionally emits `llms.txt`,
/// `sitemap.xml`, and `robots.txt`; `src/assets/**` is mirrored to
/// `out/_assets/**`; and `search.json` + `search.bin` (a docs-search
/// index) are emitted for client-side search.
///
/// # Errors
/// I/O failures on the tree walk or output creation.
pub fn compile_site(src: &Path, out: &Path) -> io::Result<SiteReport> {
    compile_site_with(src, out, None)
}

/// [`compile_site`] with a base-path override (subpath hosting: the
/// preview deploy under `/solid-fx/preview` passes that prefix here and
/// every chrome URL follows it).
///
/// # Errors
/// Same as [`compile_site`].
pub fn compile_site_with(
    src: &Path,
    out: &Path,
    base_path_override: Option<&str>,
) -> io::Result<SiteReport> {
    let mut config = SiteConfig::load(src);
    if let Some(bp) = base_path_override {
        if let Some(c) = config.as_mut() {
            c.base_path = bp.trim_end_matches('/').to_string();
        }
    }
    let config = config;
    let tools = compress::Tools::detect();
    let mut report = SiteReport {
        configured: config.is_some(),
        ..SiteReport::default()
    };

    // Pass 1: render every page body and collect its metadata.
    let mut pages: Vec<PageBuild> = Vec::new();
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
                build_page(src, &path, config.as_ref(), &mut report, &mut pages);
            }
        }
    }
    pages.sort_by(|a, b| a.route.cmp(&b.route));

    // Site assets: src/assets/** mirrors to out/_assets/** (the island
    // base URL space), and the chrome stylesheet is emitted first so
    // every wrapped page can link it.
    let chrome_css = out.join("_assets").join("layout.css");
    std::fs::create_dir_all(
        chrome_css
            .parent()
            .map_or_else(|| Path::new("."), Path::new),
    )?;
    std::fs::write(&chrome_css, chrome::LAYOUT_CSS)?;
    report.artifacts.push(chrome_css);
    let assets_src = src.join("assets");
    if assets_src.is_dir() {
        report.assets_copied = copy_tree(&assets_src, &out.join("_assets"))?;
    }

    // Pass 2: chrome-wrap and write every page, in route order. The
    // search corpus grows alongside (title/url/description/body JSON for
    // the doc-search island, plus the docs-search binary index).
    let base_path_str = config.as_ref().map(|c| c.base_path.clone());
    let mut corpus_entries: Vec<String> = Vec::new();
    let mut index = docs_search::IndexBuilder::new();
    for page in &pages {
        // Breadcrumbs: Home + the intermediate directory (no link unless
        // a group index page exists - the styled 404 catches stale ones).
        let segs: Vec<&str> = page.route.split('/').collect();
        // document() owns the Home crumb; intermediate directories are
        // plain-text labels here (group index pages may not exist).
        let mut breadcrumbs: Vec<(String, Option<String>)> = Vec::new();
        if let Some(dir) = segs.get(1) {
            let mut label = dir.split('.').next().unwrap_or(dir).to_string();
            if let Some(first) = label.chars().next() {
                label = first.to_uppercase().to_string() + &label[first.len_utf8()..];
            }
            breadcrumbs.push((label, None));
        }
        let chrome = Chrome {
            nav: nav_groups(&pages, &page.route),
            active_route: page.route.clone(),
            toc: chrome::extract_toc(&page.body),
            prev: prev_of(&pages, &page.route),
            next: next_of(&pages, &page.route),
            search_island: config.as_ref().and_then(|c| c.search_island.clone()),
            search_index: Some(match base_path_str.as_deref() {
                Some("") | None => "/search.json".to_string(),
                Some(bp) => format!("{bp}/search.json"),
            }),
            search_wasm: config.as_ref().and_then(|c| c.search_wasm.clone()),
            search_bin: config.as_ref().and_then(|c| c.search_bin.clone()),
        };
        let doc = seo::document(
            &page.body,
            &page.meta,
            &page.title,
            &page.description,
            &page.route,
            config.as_ref(),
            &breadcrumbs,
        );
        let site_title = config
            .as_ref()
            .map_or_else(|| "Docs".to_string(), |c| c.site_title.clone());
        let with_chrome = chrome::page(
            &doc,
            &site_title,
            &chrome,
            base_path_str.as_deref().unwrap_or(""),
        );
        let target = out.join(&page.route);
        if let Some(parent) = target.parent() {
            std::fs::create_dir_all(parent)?;
        }
        std::fs::write(&target, with_chrome)?;
        report.built.push(page.source.clone());
        report.artifacts.push(target);
        report.pages.push(SeoPage {
            route: page.route.clone(),
            title: page.title.clone(),
            description: page.description.clone(),
            date: page.date.clone(),
        });

        corpus_entries.push(format!(
            "{{\"title\":{},\"url\":{},\"description\":{},\"body\":{}}}",
            serde_json::to_string(&page.title).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&format!("/{}", page.route)).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&page.description).unwrap_or_else(|_| "\"\"".into()),
            serde_json::to_string(&page.text).unwrap_or_else(|_| "\"\"".into()),
        ));
        let _ = index.add_doc(&format!(
            "{} {} {}",
            page.title, page.description, page.text
        ));
    }

    // SEO artifacts.
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
        let feed_path = out.join("atom.xml");
        std::fs::write(&feed_path, seo::atom_xml(config, page_refs))?;
        report.artifacts.push(feed_path);
    }

    // Styled 404: a chrome-wrapped page served for missing routes.
    let not_found = seo::document(
        "<h1>404</h1><p>This page does not exist. Use the navigation to find your way back.</p>",
        &seo::PageMeta::default(),
        "404 - not found",
        "Page not found.",
        "404.html",
        config.as_ref(),
        &[],
    );
    let not_found = chrome::page(
        &not_found,
        &config
            .as_ref()
            .map_or_else(|| "Docs".to_string(), |c| c.site_title.clone()),
        &chrome::Chrome {
            nav: nav_groups(&pages, "404.html"),
            active_route: "404.html".into(),
            toc: Vec::new(),
            prev: None,
            next: None,
            search_island: config.as_ref().and_then(|c| c.search_island.clone()),
            search_index: Some(match base_path_str.as_deref() {
                Some("") | None => "/search.json".to_string(),
                Some(bp) => format!("{bp}/search.json"),
            }),
            search_wasm: config.as_ref().and_then(|c| c.search_wasm.clone()),
            search_bin: config.as_ref().and_then(|c| c.search_bin.clone()),
        },
        base_path_str.as_deref().unwrap_or(""),
    );
    let not_found_path = out.join("404.html");
    std::fs::write(&not_found_path, not_found)?;
    report.artifacts.push(not_found_path);

    // Search artifacts: the JSON corpus for the doc-search island and a
    // docs-search binary index for the future in-browser reader.
    if !corpus_entries.is_empty() {
        let corpus = format!("[\n{}\n]\n", corpus_entries.join(",\n"));
        let corpus_path = out.join("search.json");
        std::fs::write(&corpus_path, corpus)?;
        report.artifacts.push(corpus_path);
        let index_path = out.join("search.bin");
        match index.build() {
            Ok(bytes) => {
                std::fs::write(&index_path, bytes)?;
                report.artifacts.push(index_path);
            }
            Err(e) => report
                .warnings
                .push(format!("search index build failed: {e}")),
        }
    }

    precompress(out, &tools, &mut report);
    Ok(report)
}

/// Build one page's metadata and body into the pending list. All
/// failures land in the report - one bad page never blocks the site.
fn build_page(
    src: &Path,
    path: &Path,
    config: Option<&SiteConfig>,
    report: &mut SiteReport,
    pages: &mut Vec<PageBuild>,
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
    let ctx = match config {
        Some(c) if !c.base_path.is_empty() => {
            BuildContext::with_asset_base(root, format!("{}/_assets", c.base_path))
        }
        _ => BuildContext::new(root),
    };
    let body = match compile(&body_markdown, &ctx) {
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

    pages.push(PageBuild {
        source: path.to_path_buf(),
        route,
        title,
        description,
        date: meta.date.clone(),
        meta,
        text: crate::seo::plain_text(&body),
        body,
    });
}

/// Sidebar nav groups: group pages by their first route segment, label
/// groups by the second segment when present ("content/TextAnimations"
/// -> "TextAnimations"). The homepage stays out - the header brand
/// links it.
fn nav_groups(pages: &[PageBuild], active_route: &str) -> Vec<NavGroup> {
    let mut order: Vec<String> = Vec::new();
    let mut groups: std::collections::HashMap<String, Vec<(String, String)>> =
        std::collections::HashMap::new();
    for page in pages {
        if page.route == "index.html" {
            continue;
        }
        // Group key: the directory the page lives in. Depth-2 routes
        // ("guides/one.html") group under their top segment; deeper
        // routes group under the first two ("extras/osint"), so
        // categories stay separate.
        let segs: Vec<&str> = page.route.split('/').collect();
        let key = match (segs.first(), segs.get(1)) {
            (Some(top), Some(second)) if second.contains('.') => (*top).to_string(),
            (Some(top), Some(second)) => format!(
                "{top}/{stem}",
                stem = second.split('.').next().unwrap_or(second)
            ),
            _ => "pages".to_string(),
        };
        if !groups.contains_key(&key) {
            order.push(key.clone());
        }
        groups
            .entry(key)
            .or_default()
            .push((page.route.clone(), page.title.clone()));
    }
    let mut out = Vec::new();
    for key in order {
        let items = groups.remove(&key).unwrap_or_default();
        let label = key.rsplit('/').next().unwrap_or(&key).to_string();
        let active = active_route.starts_with(&format!("{key}/"));
        out.push(NavGroup {
            label: display_label(&key, &label),
            active,
            items,
        });
    }
    out
}

/// Display label: capitalize the group's last segment for the sidebar.
fn display_label(key: &str, label: &str) -> String {
    let mut chars = label.chars();
    match chars.next() {
        Some(first) => {
            let mut out = String::from(key);
            out.clear();
            out.push(first.to_ascii_uppercase());
            out.push_str(chars.as_str());
            out
        }
        None => key.to_string(),
    }
}

fn prev_of(pages: &[PageBuild], route: &str) -> Option<(String, String)> {
    let pos = pages.iter().position(|p| p.route == route)?;
    let previous = pos.checked_sub(1)?;
    pages
        .get(previous)
        .map(|p| (p.route.clone(), p.title.clone()))
}

fn next_of(pages: &[PageBuild], route: &str) -> Option<(String, String)> {
    let pos = pages.iter().position(|p| p.route == route)?;
    pages
        .get(pos + 1)
        .map(|p| (p.route.clone(), p.title.clone()))
}

/// Mirror-copy a directory tree; returns the file count.
fn copy_tree(from: &Path, to: &Path) -> io::Result<usize> {
    let mut count = 0usize;
    let mut stack = vec![from.to_path_buf()];
    while let Some(dir) = stack.pop() {
        for entry in std::fs::read_dir(&dir)? {
            let entry = entry?;
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
            } else {
                let rel = path.strip_prefix(from).unwrap_or(&path).to_path_buf();
                let dest = to.join(rel);
                if let Some(parent) = dest.parent() {
                    std::fs::create_dir_all(parent)?;
                }
                std::fs::copy(&path, &dest)?;
                count += 1;
            }
        }
    }
    Ok(count)
}

/// The precompression pass (Wave 2c.5): sidecars for every text
/// artifact, plus the shared zstd dictionary and `.dcz` encodings when
/// the site has enough pages. Absent tools degrade gracefully (warning,
/// not failure) - the estate's spawn-don't-link doctrine.
fn precompress(out: &Path, tools: &compress::Tools, report: &mut SiteReport) {
    if !tools.any() {
        report
            .warnings
            .push("no gzip/brotli/zstd on PATH: precompression skipped".into());
        return;
    }
    let mut compressed = 0usize;
    let artifacts = report.artifacts.clone();
    for artifact in &artifacts {
        match compress::sidecars(tools, artifact) {
            Ok(encodings) => compressed += encodings.len(),
            Err(e) => report
                .warnings
                .push(format!("{}: compression failed: {e}", artifact.display())),
        }
    }
    let dict_path = out.join("site.dict");
    match compress::train_dictionary(tools, &artifacts, &dict_path) {
        Ok(true) => {
            report.dictionary = true;
            compressed += 1;
            for artifact in &artifacts {
                if matches!(
                    artifact.extension().and_then(|e| e.to_str()),
                    Some("html") | Some("txt") | Some("xml") | Some("json")
                ) && compress::dcz_sidecar(tools, &dict_path, artifact).unwrap_or(false)
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
        "bin" => "application/octet-stream",
        "woff2" => "font/woff2",
        _ => "text/plain; charset=utf-8",
    }
}
