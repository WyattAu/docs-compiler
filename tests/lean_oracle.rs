//! REQ-P2 oracle: real `lean` invocations against the fixture package.
//!
//! Positive: a kernel-verified theorem becomes a certificate badge.
//! Negative: a kernel-rejected file and a sorry-ed proof both fail the
//! build typed. Requires `lean` on PATH (CI installs elan; the fixture's
//! lean-toolchain pins the version).

// Test-only lifts (docs-core decoder-pattern precedent).
#![allow(clippy::expect_used, clippy::panic)]

use docs_compiler::{compile, BuildContext};
use std::path::PathBuf;

const TOOLCHAIN: &str = "leanprover/lean4:v4.35.0";

fn root_with(lean_file: &str) -> (tempfile::TempDir, PathBuf, String) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = dir.path().to_path_buf();
    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/lean");
    std::fs::copy(fixtures.join(lean_file), root.join("theo.lean")).expect("copy fixture");
    std::fs::copy(fixtures.join("lean-toolchain"), root.join("lean-toolchain"))
        .expect("copy toolchain pin");
    let doc = format!(
        "```lean-check file=\"theo.lean\" theorem=\"{theorem}\" toolchain=\"{TOOLCHAIN}\"\n```\n",
        theorem = theorem_name(lean_file)
    );
    (dir, root, doc)
}

fn theorem_name(lean_file: &str) -> &'static str {
    match lean_file {
        "no_arbitrage.lean" => "no_arbitrage_round_trip",
        "bad_claim.lean" => "bad_claim",
        "uses_sorry.lean" => "uses_sorry",
        other => panic!("unknown fixture {other}"),
    }
}

#[test]
#[ignore = "requires lean on PATH; runs in the lean-oracle CI job"]
fn positive_theorem_produces_a_real_certificate() {
    let (_guard, root, doc) = root_with("no_arbitrage.lean");
    let html = compile(&doc, &BuildContext::new(&root)).expect("lean verifies the theorem");
    assert!(html.contains("docs-proof"), "badge emitted");
    assert!(
        html.contains("\"kernel_result\":\"verified\""),
        "certificate records the kernel verdict: {html}"
    );
    assert!(
        html.contains(&docs_proof::source_hash(
            &std::fs::read_to_string(root.join("theo.lean")).expect("fixture")
        )),
        "certificate binds to the exact source bytes"
    );
}

#[test]
#[ignore = "requires lean on PATH; runs in the lean-oracle CI job"]
fn kernel_rejection_fails_the_build() {
    // `theorem bad_claim : False := trivial` is a type error: the kernel
    // rejects it, and the build must fail with the theorem named.
    let (_guard, root, doc) = root_with("bad_claim.lean");
    match compile(&doc, &BuildContext::new(&root)) {
        Err(docs_compiler::CompilerError::Lean { theorem, detail }) => {
            assert_eq!(theorem, "bad_claim");
            assert!(!detail.is_empty());
        }
        other => panic!("expected Lean failure, got {other:?}"),
    }
}

#[test]
#[ignore = "requires lean on PATH; runs in the lean-oracle CI job"]
fn sorry_is_not_a_verified_proof() {
    // Lean exits 0 on `sorry` (warning only); the compiler must still
    // fail the build: a sorry-ed declaration is not kernel-verified.
    let (_guard, root, doc) = root_with("uses_sorry.lean");
    match compile(&doc, &BuildContext::new(&root)) {
        Err(docs_compiler::CompilerError::Lean { theorem, detail }) => {
            assert_eq!(theorem, "uses_sorry");
            assert!(detail.contains("sorry"), "detail={detail}");
        }
        other => panic!("expected Lean sorry failure, got {other:?}"),
    }
}
