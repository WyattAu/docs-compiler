//! The d2 process surface: out-of-process diagram rendering (the same
//! doctrine as REQ-P1 for lean: tools are spawned, never linked). The
//! fence body is the diagram source; `d2` renders it to static SVG at
//! build time, so pages carry no diagram runtime - the static-shell
//! contract, same as math.
//!
//! Coverage note: like `lean.rs`, the happy path needs the real `d2`
//! binary, so this module sits under the coverage-ignore regex and its
//! correctness is proven by the `diagram_oracle` suite in the dedicated
//! CI job, which runs this exact code against a pinned d2 release.

use std::io::Write as _;
use std::process::{Command, Stdio};

use crate::directives::{first_lines, html_escape};
use crate::error::CompilerError;
use crate::fences::Directive;
use crate::BuildContext;

/// `d2 [layout=...] [caption="..."]` + body.
///
/// Spawns `d2 -` (stdin → SVG on stdout). A missing binary, nonzero
/// exit, or diagnostics all fail the build naming the fence. The SVG is
/// emitted verbatim inside a figure: it is build-time tool output from a
/// first-party source, the same trust level as lean's certificate HTML.
pub(crate) fn diagram(d: &Directive, _ctx: &BuildContext) -> Result<String, CompilerError> {
    let mut command = Command::new("d2");
    command
        .arg("-")
        .stdin(Stdio::piped())
        .stdout(Stdio::piped())
        .stderr(Stdio::piped());
    if let Some(layout) = d.attr("layout") {
        command.arg("--layout").arg(layout);
    }
    let mut child = command.spawn().map_err(|source| CompilerError::Diagram {
        detail: format!("cannot spawn `d2` (is it on PATH?): {source}"),
    })?;
    // The body is small (a docs diagram); a broken pipe here means d2
    // died early, and wait_with_output surfaces its stderr below.
    let stdin_result = match child.stdin.take() {
        Some(mut stdin) => stdin
            .write_all(d.body.as_bytes())
            .map_err(|e| e.to_string()),
        None => Err("d2 stdin was already taken".to_string()),
    };
    let output = child
        .wait_with_output()
        .map_err(|source| CompilerError::Diagram {
            detail: format!("d2 did not run to completion: {source}"),
        })?;
    let stderr = String::from_utf8_lossy(&output.stderr);
    if let Err(write_error) = stdin_result {
        return Err(CompilerError::Diagram {
            detail: format!("writing the diagram source failed: {write_error}"),
        });
    }
    if !output.status.success() {
        return Err(CompilerError::Diagram {
            detail: format!("d2 exited with {}: {}", output.status, first_lines(&stderr)),
        });
    }
    let svg = String::from_utf8(output.stdout).map_err(|_| CompilerError::Diagram {
        detail: "d2 produced non-UTF-8 output".to_string(),
    })?;
    if svg.trim().is_empty() {
        return Err(CompilerError::Diagram {
            detail: "d2 produced no output".to_string(),
        });
    }
    // The substituted HTML must be ONE raw-HTML block: pulldown-cmark
    // ends the block at the first blank line, and everything after is
    // re-parsed as markdown (d2's tab-indented CSS turns into indented
    // code blocks). The SVG's layout does not depend on blank lines, so
    // they collapse away.
    let svg = svg.trim().replace("\n\n", "\n");

    let caption = d
        .attr("caption")
        .map(|c| format!("<figcaption>{}</figcaption>", html_escape(c)))
        .unwrap_or_default();
    Ok(format!(
        "<figure class=\"docs-diagram\" data-tool=\"d2\">{svg}{caption}</figure>"
    ))
}
