//! Wave 2e oracle: the search island props under subpath hosting. The
//! 2026-11-21 wasm-consumer work caught search_index being prefixed
//! twice (serve.rs + chrome.rs) and the wasm/bin URLs double-slashing -
//! both emitted 404 URLs that silently killed production search.

#![allow(clippy::indexing_slicing, clippy::expect_used, clippy::panic)]

use docs_compiler::{chrome_page, Chrome, NavGroup};

fn sample_chrome() -> Chrome {
    Chrome {
        nav: vec![NavGroup {
            label: "G".into(),
            active: false,
            items: vec![],
        }],
        active_route: String::new(),
        toc: vec![],
        prev: None,
        next: None,
        search_island: Some("/_assets/islands/seo-doc-search.js".into()),
        search_index: Some("/search.json".into()),
        search_wasm: Some("/_assets/islands/docs_search.wasm".into()),
        search_bin: Some("/search.bin".into()),
        accent: None,
    }
}

#[test]
fn search_props_are_single_prefixed_under_a_base_path() {
    let page = chrome_page(
        "<html><head><title>t</title></head><body><p>x</p></body></html>",
        "S",
        &sample_chrome(),
        "/solid-fx",
    );
    let start = page.find("data-island-props>").expect("props");
    let end = page[start..].find("</script>").expect("end");
    let props = &page[start + "data-island-props>".len()..start + end];
    assert_eq!(
        props,
        "{\"index\":\"/solid-fx/search.json\",\"wasm\":\"/solid-fx/_assets/islands/docs_search.wasm\",\"bin\":\"/solid-fx/search.bin\"}"
    );
}

#[test]
fn search_props_resolve_at_the_root_without_a_base_path() {
    let page = chrome_page(
        "<html><head><title>t</title></head><body><p>x</p></body></html>",
        "S",
        &sample_chrome(),
        "",
    );
    assert!(
        page.contains("\"index\":\"/search.json\""),
        "root hosting: no prefix"
    );
    assert!(
        page.contains("\"wasm\":\"/_assets/islands/docs_search.wasm\""),
        "root hosting: wasm unprefixed"
    );
}

#[test]
fn json_only_site_omits_wasm_props() {
    let mut chrome = sample_chrome();
    chrome.search_wasm = None;
    chrome.search_bin = None;
    let page = chrome_page(
        "<html><head><title>t</title></head><body><p>x</p></body></html>",
        "S",
        &chrome,
        "/x",
    );
    let start = page.find("data-island-props>").expect("props");
    let end = page[start..].find("</script>").expect("end");
    let props = &page[start + "data-island-props>".len()..start + end];
    assert_eq!(props, "{\"index\":\"/x/search.json\"}");
}
