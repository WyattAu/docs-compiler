//! Wave 2c.5 oracles: encoding negotiation and the precompression pass.
//! gzip exists everywhere; brotli/zstd assertions are gated on tool
//! presence so CI runners without them stay green.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use docs_compiler::{compile_site, pick_encoding, Tools};
use std::path::Path;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdirs");
    }
    std::fs::write(path, content).expect("write");
}

#[test]
fn negotiation_prefers_dictionary_then_brotli_then_zstd_then_gzip() {
    let dir = tempfile::tempdir().expect("tempdir");
    let page = dir.path().join("p.html");
    write(&page, "x");

    // Nothing available: identity.
    assert!(pick_encoding("gzip, br, zstd, dcz", &page).is_none());

    write(&page.with_extension("html.gz"), "g");
    let (token, path) = pick_encoding("gzip", &page).expect("gz served");
    assert_eq!(token, "gzip");
    assert!(path.ends_with("p.html.gz"));

    write(&page.with_extension("html.zst"), "z");
    let (token, _) = pick_encoding("gzip, zstd", &page).expect("zstd wins over gzip");
    assert_eq!(token, "zstd");

    write(&page.with_extension("html.br"), "b");
    let (token, _) = pick_encoding("gzip, br, zstd", &page).expect("br wins");
    assert_eq!(token, "br");

    write(&page.with_extension("html.dcz"), "d");
    let (token, _) = pick_encoding("gzip, br, zstd, dcz", &page).expect("dcz wins");
    assert_eq!(token, "dcz");

    // q-values are ignored by design (documented limitation): presence only.
    let (token, _) =
        pick_encoding("br;q=0.5, gzip;q=1.0", &page).expect("presence, not arithmetic");
    assert_eq!(token, "br");

    // A non-offered encoding serves identity.
    assert!(
        pick_encoding("deflate", &page).is_none(),
        "deflate: identity, no sidecar"
    );
}

#[test]
fn site_build_precompresses_and_trains_the_dictionary() {
    let tools = Tools::detect();
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    write(
        &src.join("site.toml"),
        "base_url = \"https://docs.example.com\"\n",
    );
    for n in 0..10 {
        write(
            &src.join(format!("p{n}.md")),
            &format!("# Page {n}\n\nBody {n} with some shared-ish words.\n"),
        );
    }

    let report = compile_site(&src, &out).expect("walks");
    assert_eq!(report.built.len(), 10);
    assert!(report.compressed > 0, "sidecars produced: {report:?}");
    // gzip exists on every host; its sidecars must always exist.
    assert!(out.join("p0.html.gz").is_file(), "gzip sidecar");
    if tools.zstd {
        assert!(out.join("p0.html.zst").is_file(), "zstd sidecar");
    }
    if tools.brotli {
        assert!(out.join("p0.html.br").is_file(), "brotli sidecar");
    }

    if tools.zstd {
        assert!(
            report.dictionary,
            "ten pages train a dictionary: {report:?}"
        );
        assert!(out.join("site.dict").is_file(), "dictionary emitted");
        // dcz encodings exist for html/txt/xml artifacts.
        assert!(out.join("p0.html.dcz").is_file(), "dcz sidecar for pages");
        if report.configured {
            assert!(out.join("llms.txt.dcz").is_file(), "dcz for text artifacts");
        }
    } else {
        assert!(!report.dictionary, "no zstd, no dictionary");
        assert!(
            report.warnings.iter().any(|w| w.contains("dictionary")),
            "the skip is reported: {:?}",
            report.warnings
        );
    }
}

#[test]
fn tiny_sites_skip_the_dictionary_with_a_reported_warning() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    write(&src.join("only.md"), "# Only\n");
    let report = compile_site(&src, &out).expect("walks");
    assert!(!report.dictionary, "one page cannot train");
    assert!(
        report.warnings.iter().any(|w| w.contains("dictionary")),
        "skip is reported: {:?}",
        report.warnings
    );
}
