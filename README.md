# docs-compiler

The composition CLI for the estate's docs crates (docs-core extension
program, `docs-core/SPEC.md` M3/M4): scan a markdown document for the
four directive kinds, resolve each through the crate that owns it,
substitute raw HTML, and render the page with docs-core. Any failing
directive fails the build with a typed error naming its source; nothing
degrades to a silent placeholder.

## Directive fences

| Fence | Resolved by | Emitted |
|---|---|---|
| `include-symbol file=... symbol=...` | docs-symbols | `<figure class="docs-symbol" data-anchor="...">` with the byte-exact span |
| `witness wasm=... [fuel=...] [assert_exit=...] [assert_stdout=...]` | docs-witness | `<div class="docs-witness" data-exit data-fuel>` with captured stdout |
| `lean-check file=... theorem=... toolchain=...` | out-of-process `lean` + docs-proof | proof badge + `<script type="application/json" data-proof-certificate>` |
| `island ...` | docs-core (native) | `<docs-island>` (astro-island-compatible) |

## Guarantees (M3/M4, honest)

- **Build-time resolution**: all four kinds resolve during compilation;
  the emitted page is static HTML. Lean is spawned as an external
  process (REQ-P1: never linked).
- **Real kernel verdicts or no badge** (REQ-P2): nonzero exit, kernel
  errors, or Lean's `declaration uses \`sorry\`` diagnostic (which exits
  0 - it is caught) fail the build with the theorem named. The oracle
  suite runs real lean against the fixture package (positive, kernel
  rejection, sorry).
- **Certificates bind bytes** (REQ-P3): the badge embeds the SHA-256 of
  the exact Lean source; docs-proof's verifier re-checks on every build.
- **Fences are CommonMark-fenced**: an opening fence whose info string
  contains backticks is not a directive (it would swallow the rest of
  the file); unknown fences pass through as plain code.

## CLI

```
docs-compiler <input.md> [-o out.html] [--root dir]
```

Directive-relative paths resolve against `--root` (default: the input's
directory).

## Where the truth lives

| Concern | Truth | Path |
|---|---|---|
| Directive grammar + fence resolution | source of record | `src/directives.rs`, `src/fences.rs` |
| Diagram rendering (d2 -> static SVG) | source of record | `src/d2.rs` |
| Dev server (site compile, routing, MIME) | source of record | `src/serve.rs` |
| SEO emission (head meta, JSON-LD, llms.txt, sitemap, robots) | source of record | `src/seo.rs` |
| Lean invocation (out-of-process, REQ-P1) | source of record | `src/lean.rs` |
| Typed failures | source of record | `src/error.rs` |
| Program SPEC, crate topology, milestones | SPEC | `docs-core/SPEC.md` |
| Layer declaration + lean-oracle CI job | manifest + workflow | `Cargo.toml`, `.github/workflows/ci.yml` |

## Layer

L3 - composition: docs-core (L2) + docs-symbols/docs-witness/docs-proof
(L0/L1) + clap. `lean` is spawned, not linked.

## Tests

`cargo test` under clippy `-D warnings` (denies lifted in test targets
only): fence scanning and attribute parsing, the end-to-end four-kind
page (M3/M4 close criterion), typed failures per directive, and the
`lean_oracle` suite, which requires `lean` on PATH (CI installs elan
with the pinned `leanprover/lean4:v4.35.0`).

Licensed under MIT OR Apache-2.0.
