# Security Policy

## Reporting a vulnerability

Email wyatt_au@protonmail.com with the repo name in the subject. Please
include reproduction steps; do not open a public issue for an unfixed
vulnerability.

## Content discipline

Everything a directive names - source files, wasm modules, Lean files -
is repository content, still untrusted input. Snippet execution is
delegated to docs-witness (fresh wasmtime engine, fuel-bounded, no
fs/sockets); symbol extraction to docs-symbols (total, typed errors).
The `lean` process runs on repository fixture files with no network and
no environment beyond the toolchain; a failing or sorry-ed proof fails
the build, it is never emitted as a badge. Fence attributes are parsed
by a bounded scanner; a malformed fence fails the build rather than
swallowing the document. If you find an input that panics, hangs, or
emits a badge for an unverified claim, that is a security bug.

## Trust model

The proof badge is as trustworthy as the toolchain provenance recorded
in it; the compiler pins the fixture toolchain
(`leanprover/lean4:v4.35.0`) and docs-proof re-verifies the source hash
on every build.

## Supported versions

Only the latest released `0.1.x` is supported with fixes.
