//! Wave 2 dev-server oracles: site compilation, routing, MIME.

#![allow(
    clippy::unwrap_used,
    clippy::expect_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use docs_compiler::{compile_site, mime_for, resolve_route};
use std::path::Path;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdirs");
    }
    std::fs::write(path, content).expect("write");
}

#[test]
fn compile_site_builds_every_page_and_collects_one_bad_page() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    write(&src.join("index.md"), "# Home\n\nhello\n");
    write(
        &src.join("guides/second.md"),
        "## Second\n\n```island nav client=load\n{}\n```\n",
    );
    // A malformed island fence fails ONE page, not the site.
    write(&src.join("broken.md"), "```island\n{}\n```\n");

    let report = compile_site(&src, &out).expect("walks");
    assert_eq!(report.built.len(), 2, "two good pages: {:?}", report);
    assert_eq!(
        report.errors.len(),
        1,
        "the broken page is named: {:?}",
        report
    );
    assert!(report.errors[0].0.ends_with("broken.md"));
    assert!(
        report.errors[0].1.contains("island"),
        "error names the fence: {:?}",
        report
    );

    assert!(out.join("index.html").is_file());
    assert!(out.join("guides/second.html").is_file());
    let html = std::fs::read_to_string(out.join("guides/second.html")).expect("read");
    assert!(
        html.contains("docs-island"),
        "islands flow through site build: {html}"
    );
}

#[test]
fn routes_resolve_indexes_html_fallbacks_and_reject_traversal() {
    let dir = tempfile::tempdir().expect("tempdir");
    let out = dir.path();
    write(&out.join("index.html"), "home");
    write(&out.join("guides/x.html"), "x");
    write(&out.join("plain.txt"), "t");

    assert_eq!(resolve_route(out, "/"), Some(out.join("index.html")));
    assert_eq!(
        resolve_route(out, "/index.html"),
        Some(out.join("index.html"))
    );
    assert_eq!(
        resolve_route(out, "/guides/x.html?cache=1#frag"),
        Some(out.join("guides/x.html"))
    );
    assert_eq!(
        resolve_route(out, "/guides/x"), // extensionless falls back to .html
        Some(out.join("guides/x.html"))
    );
    assert_eq!(resolve_route(out, "/missing"), None);
    assert_eq!(resolve_route(out, "/../secret"), None, "traversal dies");
    assert_eq!(
        resolve_route(out, "/%2e%2e/secret"),
        None,
        "encoded traversal dies"
    );
    assert_eq!(
        resolve_route(out, "/plain%2etxt"),
        Some(out.join("plain.txt")),
        "percent escapes decode"
    );
}

#[test]
fn mime_matches_the_site_surface() {
    assert_eq!(
        mime_for(Path::new("a/index.html")),
        "text/html; charset=utf-8"
    );
    assert_eq!(
        mime_for(Path::new("x.js")),
        "text/javascript; charset=utf-8"
    );
    assert_eq!(mime_for(Path::new("mod.wasm")), "application/wasm");
    assert_eq!(mime_for(Path::new("logo.svg")), "image/svg+xml");
    assert_eq!(mime_for(Path::new("data.bin")), "text/plain; charset=utf-8");
}
