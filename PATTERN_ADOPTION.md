# Pattern adoption

docs-compiler is the L3 composition CLI for the estate's docs crates.
This file declares its relationship to the pattern canon
(pattern-standards, references/adoption-format.md) and is validated by
`scripts/adoption-check.py`.

| Pattern | Status | Evidence |
|---|---|---|
| T13 | adopted | src/lib.rs:62 (`compile()` is the pure pipeline core; src/main.rs:65 is the CLI shell owning argv, the dev server, the watcher, and process spawning) |
| T49 | partial | src/lean.rs:61 (the build verifies each lean certificate against the current source and toolchain before the page ships; the crate composes the verifier half, it does not run a loop) |
