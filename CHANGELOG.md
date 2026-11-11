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
