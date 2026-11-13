# Changelog

All notable changes to this project are documented in this file.

## [0.1.0] - 2026-11-11

### Added

- Fence scanner for the three directive kinds plus pass-through for
  plain code and island fences; CommonMark guard: an info string
  containing backticks is never treated as an opening fence.
- `include-symbol` resolution through docs-symbols: byte-exact span
  HTML with the stable anchor ID.
- `witness` resolution through docs-witness: deterministic execution,
  `assert_exit` / `assert_stdout` enforcement, stdout + fuel + exit in
  the emitted block.
- `lean-check` resolution: out-of-process `lean` invocation, sorry
  detection (Lean 4.35 routes diagnostics to stdout with backticks),
  docs-proof certificate creation + self-verification, badge emission.
- `compile` library pipeline and a `docs-compiler <input> [-o] [--root]`
  CLI.
- End-to-end test with all four directive kinds (M3/M4 close criterion)
  and the `lean_oracle` suite (REQ-P2 oracle: positive, kernel
  rejection, sorry) against the pinned `leanprover/lean4:v4.35.0`
  fixture package.

## [0.2.0] - 2026-11-13

### Added

- **Dev server (Wave 2):** `docs-compiler serve <src-dir>` compiles every
  markdown page under a directory and serves the result, recompiling on
  any source change (notify, debounced). One bad page reports and the
  site keeps serving the last good HTML - fail-loud, not fail-dead.
  Routing: `index.html` for directory roots, extensionless `.html`
  fallback, percent-decoding, traversal rejection. Binds 0.0.0.0 by
  default so phones on the LAN can open the preview (verified on-device
  via adb: build-time MathML renders in mobile Chrome with zero client
  JS). The reusable half (site compile, routing, MIME) lives in the
  library and is unit-tested; the socket loop and watcher stay in the
  binary.

### Changed

- **Breaking (CLI):** `docs-compiler <input>` became
  `docs-compiler compile <input>` (plus the new `serve` subcommand).
- Dependency floor: `docs-core = "0.2"`, plus CLI-only `tiny_http`,
  `notify` (docs-core itself stays L2 with no web framework - the
  server is a compiler concern, recorded in the ledger as W2-3).

## [0.3.0] - 2026-11-13

### Added

- **Diagrams (Wave 2b):** the `d2` fence renders Terrastruct D2 source
  to static SVG at build time - `d2` is spawned out-of-process (the
  REQ-P1 doctrine: tools are never linked), the SVG embeds verbatim in
  a `figure.docs-diagram`, and pages carry no diagram runtime.
  Attributes: `layout=` (passed as `--layout`) and `caption=`
  (figcaption). A missing binary, nonzero exit, or empty output fails
  the build naming the fence (`CompilerError::Diagram`).
- `Directive` now carries the fence `body` verbatim (body-driven
  directives like `d2`; file-reading directives ignore it).
- New `diagram-oracle` CI job (mirrors lean-oracle): installs d2 v0.9.0
  pinned, runs the ignored `diagram_oracle` suite serially (the
  missing-binary test mutates the process PATH).
