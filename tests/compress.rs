//! Wave 2c.5 oracles: encoding negotiation and the precompression pass.
//! gzip exists everywhere; brotli/zstd assertions are gated on tool
//! presence so CI runners without them stay green.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use docs_compiler::{compile_site, pick_encoding, sidecar_path, sidecars, Tools};
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

/// gzip must not embed mtimes: two compressions of the same bytes
/// (files with deliberately different mtimes) produce byte-identical
/// sidecars, or the site is not byte-reproducible. The 2026-11-20 flip
/// audit caught exactly this: 479 gz sidecars differed between two
/// builds of the same source because `gzip -9` recorded the file
/// mtime in the header.
#[test]
fn gz_sidecars_are_deterministic() {
    let dir = std::env::temp_dir().join(format!("docs-compiler-det-{}", std::process::id()));
    std::fs::create_dir_all(&dir).expect("tmp dir");
    let a = dir.join("a.html");
    let b = dir.join("b.html");
    std::fs::write(&a, b"<p>same</p>").expect("write a");
    std::fs::write(&b, b"<p>same</p>").expect("write b");
    // 2020-01-01 vs 2024-06-01, deliberately far apart.
    for (p, secs) in [(&a, 1_577_836_800), (&b, 1_717_200_000)] {
        let f = std::fs::OpenOptions::new()
            .write(true)
            .open(p)
            .expect("open");
        let t = std::time::UNIX_EPOCH + std::time::Duration::from_secs(secs);
        f.set_times(std::fs::FileTimes::new().set_modified(t))
            .expect("utime");
    }
    let tools = Tools::detect();
    if !tools.gzip {
        eprintln!("gzip absent; determinism test skipped");
        return;
    }
    sidecars(&tools, &a).expect("sidecars a");
    sidecars(&tools, &b).expect("sidecars b");
    let ga = std::fs::read(sidecar_path(&a, "gz")).expect("gz a");
    let gb = std::fs::read(sidecar_path(&b, "gz")).expect("gz b");
    assert_eq!(ga, gb, "gzip sidecars must not depend on file mtime");
}
