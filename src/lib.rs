//! docs-compiler: the composition CLI for the estate's docs crates
//! (docs-core extension program, SPEC.md M3/M4).
//!
//! The pipeline is small and honest: scan a markdown document for the
//! four directive kinds, resolve each one through the crate that owns it
//! (`include-symbol` through docs-symbols, `witness` through
//! docs-witness, `lean-check` through an out-of-process `lean` plus
//! docs-proof, islands natively through docs-core), substitute the
//! resulting raw HTML, and render the page with docs-core. Any failing
//! directive fails the build with a typed error naming its source;
//! nothing degrades to a silent placeholder.
//!
//! ```
//! use docs_compiler::BuildContext;
//!
//! // No directives: the document flows through docs-core untouched.
//! let html = docs_compiler::compile(
//!     "# Title\n\nSome *prose*.\n",
//!     &BuildContext::new("."),
//! )
//! .unwrap();
//! assert!(html.contains("<h1>Title</h1>"));
//! ```
//!
//! # Layer
//!
//! L3 - composition. Depends on docs-core (L2) and the capability
//! crates (L0/L1); spawns `lean` as an external process (REQ-P1).

#![deny(missing_docs)]
#![forbid(unsafe_code)]

mod d2;
mod directives;
mod error;
mod fences;
mod lean;
mod seo;
mod serve;

pub use directives::{resolve, BuildContext};
pub use error::CompilerError;
pub use fences::{scan, Directive, DirectiveKind, Scanned};
pub use seo::{
    document, first_h1, first_paragraph, llms_txt, robots_txt, sitemap_xml, split_front_matter,
    PageMeta, SeoPage, SiteConfig,
};
pub use serve::{compile_site, mime_for, resolve_route, SiteReport};

/// Compile one markdown document into a static HTML page.
///
/// Directive fences (`include-symbol`, `witness`, `lean-check`) resolve
/// through the owning crate and become raw HTML; island fences and all
/// other markdown render through docs-core (M1 contract unchanged).
pub fn compile(source: &str, ctx: &BuildContext) -> Result<String, CompilerError> {
    let scanned = scan(source);
    let resolved = scanned.substitute(|d| resolve(d, ctx))?;
    docs_core::render(&resolved, "/_assets").map_err(CompilerError::Render)
}

/// The crate's version, for callers that report it.
pub const VERSION: &str = env!("CARGO_PKG_VERSION");
