//! M3/M4 close criteria: end-to-end page builds with all four directive
//! kinds (island, include-symbol, witness, lean-check).

// Test-only lifts (docs-core decoder-pattern precedent).
#![allow(clippy::expect_used, clippy::panic)]

use docs_compiler::{compile, BuildContext};
use std::path::PathBuf;

const TOOLCHAIN: &str = "leanprover/lean4:v4.35.0";

/// A build root with everything a four-directive document needs.
fn setup_root() -> (tempfile::TempDir, PathBuf) {
    let dir = tempfile::tempdir().expect("tempdir");
    let root: PathBuf = dir.path().to_path_buf();

    std::fs::write(
        root.join("sample.rs"),
        "pub mod engine {\n    pub fn spin_up() -> bool { true }\n}\n",
    )
    .expect("sample.rs");

    let wat = r#"
    (module
      (import "wasi_snapshot_preview1" "fd_write"
        (func $fd_write (param i32 i32 i32 i32) (result i32)))
      (memory (export "memory") 1)
      (data (i32.const 0) "\10\00\00\00\05\00\00\00")
      (data (i32.const 16) "hello")
      (func (export "_start")
        i32.const 1
        i32.const 0
        i32.const 1
        i32.const 48
        call $fd_write
        drop))
    "#;
    std::fs::write(root.join("hello.wasm"), wat::parse_str(wat).expect("wat")).expect("wasm");

    let fixtures = PathBuf::from(env!("CARGO_MANIFEST_DIR")).join("test-fixtures/lean");
    for name in [
        "no_arbitrage.lean",
        "bad_claim.lean",
        "uses_sorry.lean",
        "lean-toolchain",
    ] {
        std::fs::copy(fixtures.join(name), root.join(name)).expect("copy lean fixture");
    }
    (dir, root)
}

/// The whole document: all four directive kinds plus plain markdown.
fn document() -> String {
    format!(
        r#"# Program page

Plain prose first.

```island nav-search client=visible
{{"count": 3}}
```

```include-symbol file="sample.rs" symbol="engine::spin_up"
```

```witness wasm="hello.wasm" assert_stdout="hello" assert_exit=0
```

```lean-check file="no_arbitrage.lean" theorem="no_arbitrage_round_trip" toolchain="{toolchain}"
```

Closing prose.
"#,
        toolchain = TOOLCHAIN
    )
}

#[test]
#[ignore = "the lean-check fence needs lean on PATH; the four-kind e2e runs in the lean-oracle CI job"]
fn end_to_end_page_with_all_four_directive_kinds() {
    let (_guard, root) = setup_root();
    let html = compile(&document(), &BuildContext::new(&root)).expect("builds");

    // Island (docs-core native).
    assert!(html.contains("<docs-island"), "island emitted");
    // include-symbol (docs-symbols): anchor ID + escaped code.
    assert!(html.contains("docs-symbol"), "symbol figure emitted");
    assert!(
        html.contains("dsym-rust-"),
        "anchor id present (REQ-S3 through the pipeline)"
    );
    assert!(html.contains("pub fn spin_up"), "extracted code present");
    // witness (docs-witness): captured stdout + assertions held.
    assert!(html.contains("docs-witness"), "witness block emitted");
    assert!(html.contains("hello"), "witness stdout embedded");
    // lean-check (out-of-process lean + docs-proof certificate).
    assert!(html.contains("docs-proof"), "proof badge emitted");
    assert!(
        html.contains("data-proof-certificate"),
        "certificate JSON embedded"
    );
    assert!(html.contains("<h1>Program page</h1>"), "markdown rendered");
    assert!(html.contains("Closing prose."), "trailing markdown kept");
}

/// The lean-free subset (island + include-symbol + witness): runs in the
/// shared quality job, where elan is not installed.
#[test]
fn end_to_end_page_without_lean_directives() {
    let (_guard, root) = setup_root();
    let doc = r#"# Lean-free page

```island nav-search client=visible
{"count": 1}
```

```include-symbol file="sample.rs" symbol="engine::spin_up"
```

```witness wasm="hello.wasm" assert_stdout="hello" assert_exit=0
```

Tail prose.
"#;
    let html = compile(doc, &BuildContext::new(&root)).expect("builds");
    assert!(html.contains("<docs-island"));
    assert!(html.contains("docs-symbol") && html.contains("dsym-rust-"));
    assert!(html.contains("docs-witness") && html.contains("hello"));
    assert!(html.contains("<h1>Lean-free page</h1>"));
    assert!(!html.contains("docs-proof"), "no lean fence, no badge");
}

#[test]
fn a_failing_directive_fails_the_build_typed() {
    let (_guard, root) = setup_root();
    let doc = "before\n\n```include-symbol file=\"sample.rs\" symbol=\"engine::missing\"\n```\n";
    match compile(doc, &BuildContext::new(&root)) {
        Err(docs_compiler::CompilerError::Symbol(docs_symbols::SymbolError::Unresolvable {
            path,
            ..
        })) => assert_eq!(path, "engine::missing"),
        other => panic!("expected Symbol unresolvable, got {other:?}"),
    }
}

#[test]
fn witness_assertion_mismatch_fails_the_build() {
    let (_guard, root) = setup_root();
    let doc = "```witness wasm=\"hello.wasm\" assert_stdout=\"goodbye\"\n```\n\nprose\n";
    match compile(doc, &BuildContext::new(&root)) {
        Err(docs_compiler::CompilerError::WitnessAssert(e)) => {
            assert!(e.to_string().contains("assert_stdout"));
        }
        other => panic!("expected WitnessAssert, got {other:?}"),
    }
}
