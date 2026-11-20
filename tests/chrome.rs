//! Wave 2d oracles: the site chrome wraps pages correctly, the TOC is
//! extracted from anchored headings, and the layout stylesheet exists.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use docs_compiler::{
    accent_override, chrome_layout_css, chrome_page, extract_toc, Chrome, NavGroup, TocEntry,
};

fn sample_chrome(active_route: &str) -> Chrome {
    Chrome {
        nav: vec![
            NavGroup {
                label: "Guides".into(),
                active: active_route.starts_with("guides"),
                items: vec![
                    ("guides/setup.html".into(), "Setup".into()),
                    ("guides/usage.html".into(), "Usage".into()),
                ],
            },
            NavGroup {
                label: "Reference".into(),
                active: active_route == "reference",
                items: vec![("reference/api.html".into(), "API".into())],
            },
        ],
        active_route: active_route.into(),
        toc: vec![
            TocEntry {
                level: 2,
                anchor: "setup".into(),
                text: "Setup".into(),
            },
            TocEntry {
                level: 3,
                anchor: "options".into(),
                text: "Options".into(),
            },
        ],
        prev: Some(("guides/prev.html".into(), "Previous page".into())),
        next: Some(("guides/next.html".into(), "Next page".into())),
        search_island: Some("/_assets/islands/doc-search.js".into()),
        search_index: Some("/search.json".into()),
        search_wasm: Some("/_assets/islands/docs_search.wasm".into()),
        search_bin: Some("/search.bin".into()),
        accent: None,
    }
}

#[test]
fn chrome_page_lifts_the_body_into_the_layout() {
    let doc = "<!doctype html><html><head><title>T</title></head><body><h1 id=\"guide\">Guide</h1><p>Body.</p></body></html>";
    let page = chrome_page(doc, "Estate Docs", &sample_chrome("guides/setup.html"), "");
    assert!(
        page.contains("<header class=\"site-header\">"),
        "header: {page}"
    );
    assert!(page.contains("<nav class=\"sidebar\">"), "sidebar: {page}");
    assert!(page.contains("<main>"), "main wraps content: {page}");
    assert!(
        page.contains("<h1 id=\"guide\">Guide</h1>"),
        "content lifted intact: {page}"
    );
    assert!(
        page.contains("class=\"active\""),
        "active nav item marked: {page}"
    );
    assert!(page.contains("toc-l2"), "TOC rendered: {page}");
    assert!(page.contains("pager prev"), "prev pager: {page}");
    assert!(page.contains("pager next"), "next pager: {page}");
    assert!(
        page.contains("docs-island"),
        "search island in header: {page}"
    );
    assert!(page.contains("theme-toggle"), "theme toggle: {page}");
    assert!(page.contains("layout.css"), "stylesheet link: {page}");
}

#[test]
fn chrome_escapes_titles_into_attributes_and_markup() {
    let mut chrome = sample_chrome("guides");
    chrome.nav[0].items[0].1 = "Setup\" onload=\"x".into();
    let doc = "<!doctype html><html><head></head><body><p>x</p></body></html>";
    let page = chrome_page(doc, "Site", &chrome, "");
    assert!(!page.contains("onload=\"x\""), "titles escaped: {page}");
}

#[test]
fn toc_extraction_reads_anchored_headings_only() {
    let body = "<h1 id=\"guide\">Guide</h1><h2 id=\"setup\">Setup</h2><h3 id=\"options\">Options</h3><h4>plain</h4><p>text</p>";
    let toc = extract_toc(body);
    assert_eq!(toc.len(), 2, "h2 + h3 listed, h1/h4 not: {toc:?}");
    assert_eq!(toc[0].anchor, "setup");
    assert_eq!(toc[0].level, 2);
    assert_eq!(toc[1].level, 3);
}

#[test]
fn layout_css_is_embedded() {
    assert!(
        chrome_layout_css.contains(".sidebar"),
        "sidebar styles present"
    );
    assert!(
        chrome_layout_css.contains("data-theme"),
        "light palette present"
    );
}

#[test]
fn accent_override_six_digit_hex() {
    let css = accent_override("#ff5c7a").expect("valid hex");
    assert!(css.contains("--accent:#ff5c7a"), "dark accent verbatim");
    // Light variant: x0.55 per channel (255, 92, 122 -> 140, 50, 67).
    assert!(css.contains("--accent:#8c3243"), "light accent darkened");
    assert!(css.contains("rgba(255,92,122,0.14)"), "dark soft tint");
    assert!(css.contains("rgba(140,50,67,0.12)"), "light soft tint");
    assert!(css.starts_with("<style>") && css.ends_with("</style>"));
}

#[test]
fn accent_override_three_digit_shorthand() {
    let css = accent_override("#f00").expect("valid shorthand");
    assert!(css.contains("--accent:#ff0000"), "shorthand expanded");
}

#[test]
fn accent_override_invalid_is_none() {
    for bad in ["red", "", "#12345", "#1234567", "##fff", "#gggggg"] {
        assert!(accent_override(bad).is_none(), "{bad} rejected");
    }
}

#[test]
fn accent_wired_into_page_head_after_stylesheet() {
    let mut chrome = sample_chrome("index.html");
    chrome.accent = Some("#ff5c7a".into());
    let page = chrome_page(
        "<html><head><title>t</title></head><body><p>x</p></body></html>",
        "S",
        &chrome,
        "",
    );
    let link = page.find("rel=\"stylesheet\"").expect("stylesheet link");
    let style = page.find("<style>:root").expect("accent override");
    assert!(style > link, "override wins the cascade (after the link)");
    // Absent accent = no override block.
    let plain = chrome_page(
        "<html><head><title>t</title></head><body><p>x</p></body></html>",
        "S",
        &sample_chrome("index.html"),
        "",
    );
    assert!(!plain.contains("<style>:root"), "default palette untouched");
}
