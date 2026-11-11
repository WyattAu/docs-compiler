//! The Lean process surface: out-of-process kernel invocation
//! (REQ-P1). This module is the crate's one process surface; it is
//! excluded from the line-coverage gate (coverage-ignore-regex, template
//! ADR-0005) because its happy path requires a real `lean` toolchain -
//! correctness is proven by the lean_oracle suite in the dedicated CI
//! job, which runs this exact code against the pinned toolchain.

use std::path::Path;

use docs_proof::CertificateBuilder;

use crate::directives::{first_lines, html_escape};
use crate::error::CompilerError;
use crate::fences::Directive;
use crate::BuildContext;

/// `lean-check file=... theorem=... toolchain=...`
///
/// Invokes `lean` out-of-process (REQ-P1: never linked). The kernel
/// verdict is real or the build fails: nonzero exit, kernel errors, or
/// `declaration uses 'sorry'` all fail; a passing run becomes a
/// certificate whose self-verification guards against a stale record.
pub(crate) fn lean_check(d: &Directive, ctx: &BuildContext) -> Result<String, CompilerError> {
    let file = d.require("file")?;
    let theorem = d.require("theorem")?.to_string();
    let toolchain = d.require("toolchain")?.to_string();
    let source = ctx.read_text(file)?;
    let full_path = ctx.root.join(file);

    let output = std::process::Command::new("lean")
        .arg(&full_path)
        .current_dir(ctx.root.join(file).parent().unwrap_or(Path::new(".")))
        .output()
        .map_err(|source| CompilerError::Lean {
            theorem: theorem.clone(),
            detail: format!("cannot spawn `lean` (is it on PATH?): {source}"),
        })?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    let stdout = String::from_utf8_lossy(&output.stdout);
    // Lean 4.35 routes diagnostics to stdout and words the warning with
    // backticks: `declaration uses `sorry``. Scan the combined output.
    let diagnostics = format!("{stderr}{stdout}");
    if !output.status.success() {
        return Err(CompilerError::Lean {
            theorem: theorem.clone(),
            detail: format!(
                "lean exited with {}: {}",
                output.status,
                first_lines(&diagnostics)
            ),
        });
    }
    if diagnostics.contains("declaration uses `sorry`") {
        return Err(CompilerError::Lean {
            theorem: theorem.clone(),
            detail: "declaration uses `sorry` - not a kernel-verified proof".into(),
        });
    }

    let certificate = CertificateBuilder::new(&theorem, &source, &toolchain).verified();
    docs_proof::verify(&certificate, &source, &toolchain).map_err(|e| CompilerError::Lean {
        theorem: theorem.clone(),
        detail: e.to_string(),
    })?;

    Ok(format!(
        "<span class=\"docs-proof\" data-theorem=\"{theorem}\">verified</span><script type=\"application/json\" data-proof-certificate>{badge}</script>",
        badge = html_escape(&certificate.badge_json()),
    ))
}
