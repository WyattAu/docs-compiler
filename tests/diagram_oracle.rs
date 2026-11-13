//! Wave 2b oracle: real `d2` runs against diagram fences. Ignored by
//! default (the binary is not on every machine); the dedicated
//! `diagram-oracle` CI job runs this exact code against a pinned d2
//! release, mirroring the lean-oracle doctrine.

// Test-only lifts: expectations carry context; library paths keep the
// denies.
#![allow(clippy::expect_used, clippy::unwrap_used, clippy::panic)]

use docs_compiler::{compile, BuildContext};
use std::path::PathBuf;

/// A build root with nothing in it: d2 fences carry their source in the
/// body, so the context is only needed for the other directives.
fn ctx() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root = dir.path().to_path_buf();
    (dir, root)
}

#[test]
#[ignore = "needs the real d2 binary; runs in the diagram-oracle job"]
fn d2_fence_renders_static_svg() {
    let (_guard, root) = ctx();
    let html = compile(
        "# Diagrams\n\n```d2\nservers -> db: query\n```\n",
        &BuildContext::new(&root),
    )
    .expect("diagram renders");
    assert!(
        html.contains("<figure class=\"docs-diagram\" data-tool=\"d2\">"),
        "{html}"
    );
    assert!(html.contains("<svg"), "static SVG embedded: {html}");
    assert!(html.contains("query"), "diagram text survives: {html}");
    assert!(
        !html.contains("servers -> db"),
        "the d2 source must not leak: {html}"
    );
    // The substituted HTML must stay one raw-HTML block: a blank line
    // inside the SVG would end the block and re-parse the rest (d2's
    // tab-indented CSS) as markdown.
    let figure_start = html.find("<figure class=\"docs-diagram\"").expect("figure");
    let figure_end = html[figure_start..]
        .find("</figure>")
        .expect("figure closes")
        + figure_start;
    let figure = &html[figure_start..figure_end];
    assert!(
        !figure.contains("\n\n") && !figure.contains("<pre><code>"),
        "figure must survive as one raw-HTML block: {figure}"
    );
    assert!(
        figure.contains("<path") || figure.contains("<text"),
        "shapes render: {figure}"
    );
}

#[test]
#[ignore = "needs the real d2 binary; runs in the diagram-oracle job"]
fn d2_fence_supports_layout_and_caption_attributes() {
    let (_guard, root) = ctx();
    let html = compile(
        "```d2 layout=dagre caption=\"the request path\"\nx -> y\n```\n",
        &BuildContext::new(&root),
    )
    .expect("renders");
    assert!(
        html.contains("<figcaption>the request path</figcaption>"),
        "{html}"
    );
    assert!(html.contains("<svg"), "{html}");
}

#[test]
#[ignore = "needs the real d2 binary; runs in the diagram-oracle job"]
fn broken_d2_fails_the_build_with_diagnostics() {
    let (_guard, root) = ctx();
    let err = compile(
        "```d2\nthis is not [ valid d2\n```\n",
        &BuildContext::new(&root),
    )
    .expect_err("broken diagram must fail the build");
    let text = err.to_string();
    assert!(text.contains("d2"), "{text}");
}

/// Mutates the process PATH; runs only in the oracle job. PATH points at
/// nothing but a fake, non-executable `d2`, so the spawn must fail and
/// the fence must name it.
#[test]
#[ignore = "needs the real d2 binary; runs in the diagram-oracle job"]
fn missing_d2_binary_fails_with_a_path_hint() {
    let dir = tempfile::tempdir().expect("tempdir");
    let path = dir.path().to_path_buf();
    std::fs::write(path.join("d2"), b"definitely not a binary").expect("fake d2");
    let orig = std::env::var_os("PATH");
    std::env::set_var("PATH", &path);
    let (_g, root) = ctx();
    let err = compile("```d2\nx -> y\n```\n", &BuildContext::new(&root));
    if let Some(p) = orig {
        std::env::set_var("PATH", p);
    }
    let err = err.expect_err("a missing binary must fail the fence");
    let text = err.to_string();
    assert!(text.contains("d2"), "{text}");
    assert!(text.contains("spawn") || text.contains("PATH"), "{text}");
    drop(dir);
}
