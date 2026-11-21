//! Site chrome (Wave 2d): the layout around page content — header, nav
//! sidebar, table of contents, prev/next pagination, theme toggle, and
//! the search island slot. Rust renders all of it statically; the only
//! scripts are a five-line theme toggle and (optionally) the search
//! island, which rides the same `<docs-island>` seam as every other
//! interactive piece.
//!
//! The layout is deliberately restrained: a fixed sidebar built from the
//! site's directory tree, the content column capped at a readable
//! measure, a right-hand TOC on wide screens, and a dark theme by
//! default with a persisted light alternative. No framework, no build
//! step — one hand-written stylesheet.

use crate::directives::html_escape;

/// Prefix a site-relative URL with the chrome's base path. Paths may
/// carry a leading slash; exactly one separator lands between the
/// base path and the path either way.
fn site_url(base_path: &str, path: &str) -> String {
    let path = path.strip_prefix('/').unwrap_or(path);
    if base_path.is_empty() {
        return format!("/{path}");
    }
    format!("{base_path}/{path}")
}

/// The chrome stylesheet, embedded at compile time and emitted by the
/// site builder as `/_assets/layout.css`.
pub const LAYOUT_CSS: &str = include_str!("layout.css");

/// One group in the sidebar nav tree (a top-level directory).
#[derive(Debug, Clone, PartialEq)]
pub struct NavGroup {
    /// Group label (the directory's display name).
    pub label: String,
    /// Items in document order: (route, title).
    pub items: Vec<(String, String)>,
    /// Whether this group contains the active page.
    pub active: bool,
}

/// One table-of-contents entry (h2/h3 with their anchor ids).
#[derive(Debug, Clone, PartialEq)]
pub struct TocEntry {
    /// Heading level (2 or 3 — h1 is the page title).
    pub level: u8,
    /// Anchor id from docs-core's heading emission.
    pub anchor: String,
    /// Heading text.
    pub text: String,
}

/// Everything the layout needs to wrap one page.
#[derive(Debug, Clone, PartialEq)]
pub struct Chrome {
    /// Sidebar groups, in display order.
    pub nav: Vec<NavGroup>,
    /// This page's route (marks the active nav item).
    pub active_route: String,
    /// This page's TOC (h2/h3 with anchors), possibly empty.
    pub toc: Vec<TocEntry>,
    /// Previous page: (route, title).
    pub prev: Option<(String, String)>,
    /// Next page: (route, title).
    pub next: Option<(String, String)>,
    /// Search island chunk URL, when the site wires search. Emitted
    /// verbatim (a site.toml author includes any base path).
    pub search_island: Option<String>,
    /// The search corpus URL handed to the island via props. Prefixed
    /// with the base path like every other site-relative link.
    pub search_index: Option<String>,
    /// The docs_search.wasm reader module URL (site-relative; prefixed
    /// like search_index). Empty = the island runs JSON-only scoring.
    pub search_wasm: Option<String>,
    /// The FST binary index URL (site-relative; prefixed).
    pub search_bin: Option<String>,
    /// Accent color override (`#rgb`/`#rrggbb`), from `site.toml`.
    /// None = the default palette.
    pub accent: Option<String>,
}

/// The theme-toggle inline script: toggles `data-theme` on `<html>` and
/// persists the choice. The one sanctioned inline script in the chrome
/// (everything else dynamic rides islands).
const THEME_SCRIPT: &str = "<script>const h=document.documentElement;const t=document.getElementById('theme-toggle');if(t){t.addEventListener('click',()=>{const n=h.dataset.theme==='light'?'dark':'light';h.dataset.theme=n;try{localStorage.setItem('docs-theme',n)}catch(e){}});}try{const s=localStorage.getItem('docs-theme');if(s)h.dataset.theme=s}catch(e){}</script>";

/// Parse `#rgb`/`#rrggbb` hex into (r, g, b); None on anything else.
fn parse_hex_color(s: &str) -> Option<(u8, u8, u8)> {
    let h = s.strip_prefix('#')?;
    match h.len() {
        3 => {
            let mut c = [0u8; 3];
            for (slot, ch) in c.iter_mut().zip(h.chars()) {
                *slot = (ch.to_digit(16)? * 17) as u8;
            }
            Some((c[0], c[1], c[2]))
        }
        6 => {
            let mut c = [0u8; 3];
            let mut chars = h.chars();
            for slot in &mut c {
                let hi = chars.next()?.to_digit(16)?;
                let lo = chars.next()?.to_digit(16)?;
                *slot = ((hi << 4) | lo) as u8;
            }
            Some((c[0], c[1], c[2]))
        }
        _ => None,
    }
}

/// CSS `<style>` overriding the palette accent for a `site.toml`
/// `accent = "#hex"` line, emitted after `layout.css` so it wins the
/// cascade. Both themes take the accent; light gets a darkened variant
/// (x0.55 per channel) for contrast on the near-white bg, and the soft
/// tints mirror the defaults (0.14 dark / 0.12 light alpha). Invalid
/// hex = None (the default palette stands).
pub fn accent_override(accent: &str) -> Option<String> {
    let (r, g, b) = parse_hex_color(accent)?;
    let dark = format!("#{r:02x}{g:02x}{b:02x}");
    let shade = |c: u8| (f32::from(c) * 0.55) as u8;
    let light = format!("#{0:02x}{1:02x}{2:02x}", shade(r), shade(g), shade(b));
    Some(format!(
        "<style>:root,html[data-theme=\"dark\"]{{--accent:{dark};--accent-soft:rgba({r},{g},{b},0.14)}}\
html[data-theme=\"light\"]{{--accent:{light};--accent-soft:rgba({0},{1},{2},0.12)}}</style>",
        shade(r),
        shade(g),
        shade(b),
    ))
}

/// Wrap a rendered page document with the site chrome. `doc` is the
/// full `<!doctype html>...<body>content</body></html>` document from
/// `seo::document`; the body content is lifted into the layout
/// (header + sidebar + main + TOC + pager) by exact-markup surgery on
/// markup this crate itself emitted. `base_path` prefixes every
/// site-relative URL the chrome emits (empty string = served at root).
#[must_use]
pub fn page(doc: &str, site_title: &str, chrome: &Chrome, base_path: &str) -> String {
    let url = |path: &str| {
        if base_path.is_empty() {
            format!("/{path}")
        } else {
            format!("{base_path}/{path}")
        }
    };
    let mut nav_markup = String::from("<a class=\"brand\" href=\"");
    nav_markup.push_str(&url(""));
    nav_markup.push_str("\">");
    nav_markup.push_str(&html_escape(site_title));
    nav_markup.push_str("</a>");
    for group in &chrome.nav {
        nav_markup.push_str(&format!(
            "<details class=\"nav-group{}\"><summary>{}</summary>",
            if group.active { " open active" } else { "" },
            html_escape(&group.label),
        ));
        for (route, title) in &group.items {
            let class = if *route == chrome.active_route {
                " class=\"active\""
            } else {
                ""
            };
            nav_markup.push_str(&format!(
                "<a href=\"{r}\"{c}>{t}</a>",
                r = html_escape(&url(route)),
                c = class,
                t = html_escape(title),
            ));
        }
        nav_markup.push_str("</details>");
    }

    let toc_markup = if chrome.toc.len() >= 2 {
        let mut toc = String::from("<aside class=\"toc\"><p class=\"toc-title\">On this page</p>");
        for entry in &chrome.toc {
            toc.push_str(&format!(
                "<a href=\"#{a}\" class=\"toc-l{lvl}\">{t}</a>",
                a = html_escape(&entry.anchor),
                lvl = entry.level,
                t = html_escape(&entry.text),
            ));
        }
        toc.push_str("</aside>");
        toc
    } else {
        String::new()
    };

    let pager_markup = |link: &Option<(String, String)>,
                        class: &'static str,
                        label: &'static str| {
        match link {
            Some((route, title)) => format!(
                "<a class=\"pager {class}\" href=\"{r}\"><span class=\"pager-label\">{l}</span><span class=\"pager-title\">{t}</span></a>",
                r = html_escape(&url(route)),
                l = label,
                t = html_escape(title),
            ),
            None => String::new(),
        }
    };

    let search_header = chrome.search_island.as_ref().map_or_else(String::new, |chunk| {
        let index = chrome
            .search_index
            .clone()
            .map(|i| site_url(base_path, &i))
            .unwrap_or_else(|| url("search.json"));
        let wasm_url = chrome
            .search_wasm
            .clone()
            .map(|w| site_url(base_path, &w));
        let bin_url = chrome
            .search_bin
            .clone()
            .map(|b| site_url(base_path, &b));
        let props = match (wasm_url, bin_url) {
            (Some(w), Some(b)) => {
                format!("{{\"index\":\"{i}\",\"wasm\":\"{w}\",\"bin\":\"{b}\"}}", i = html_escape(&index), w = html_escape(&w), b = html_escape(&b))
            }
            _ => format!("{{\"index\":\"{i}\"}}", i = html_escape(&index)),
        };
        format!(
            "<docs-island component-url=\"{u}\" client=\"idle\"><script type=\"application/json\" data-island-props>{props}</script></docs-island>",
            u = html_escape(chunk),
        )
    });

    let css_link = url("_assets/layout.css");
    let _ = &css_link;
    let accent_style = chrome
        .accent
        .as_deref()
        .and_then(accent_override)
        .unwrap_or_default();

    let doc = doc.replacen(
        "</head>",
        &format!(
            "<link rel=\"stylesheet\" href=\"{}\">{accent_style}</head>",
            html_escape(&css_link)
        ),
        1,
    );
    let doc = doc.replacen(
        "<body>",
        &format!(
            "<body><header class=\"site-header\"><a class=\"brand\" href=\"{home}\">{b}</a>{search}<button id=\"theme-toggle\" aria-label=\"toggle theme\"></button></header>\
<nav class=\"sidebar\">{nav}</nav>\
<div class=\"page\"><main>",
            home = html_escape(&url("")),
            b = html_escape(site_title),
            search = search_header,
            nav = nav_markup,
        ),
        1,
    );
    doc.replacen(
        "</body></html>",
        &format!(
            "<div class=\"pager\">{prev}{next}</div></main>{toc}</div>{THEME_SCRIPT}</body></html>",
            prev = pager_markup(&chrome.prev, "prev", "Previous"),
            next = pager_markup(&chrome.next, "next", "Next"),
            toc = toc_markup,
        ),
        1,
    )
}

/// Extract the TOC (h2/h3 with anchor ids) from a rendered body. h1 is
/// the page title and stays out of the TOC; deeper levels rarely carry
/// anchors worth listing.
#[must_use]
pub fn extract_toc(body: &str) -> Vec<TocEntry> {
    let mut toc = Vec::new();
    let mut rest = body;
    loop {
        let next_pos = match (rest.find("<h2 id=\""), rest.find("<h3 id=\"")) {
            (Some(a), Some(b)) => Some(a.min(b)),
            (Some(a), None) => Some(a),
            (None, Some(b)) => Some(b),
            (None, None) => None,
        };
        let Some(pos) = next_pos else { break };
        let Some(window) = rest.get(pos..) else { break };
        let level = if window.starts_with("<h2") { 2 } else { 3 };
        let Some(id_rel) = window.find("id=\"").map(|p| p + 4) else {
            break;
        };
        let Some(end_rel) = window.get(id_rel..).and_then(|s| s.find('"')) else {
            break;
        };
        let id_end = id_rel + end_rel;
        let Some(anchor) = window.get(id_rel..id_end).map(str::to_string) else {
            break;
        };
        let Some(gt_rel) = window.get(id_end..).and_then(|s| s.find('>')) else {
            break;
        };
        let text_start = id_end + gt_rel + 1;
        let Some(lt_rel) = window.get(text_start..).and_then(|s| s.find('<')) else {
            break;
        };
        let text_end = text_start + lt_rel;
        let Some(text) = window
            .get(text_start..text_end)
            .map(|s| s.trim().to_string())
        else {
            break;
        };
        if !text.is_empty() {
            toc.push(TocEntry {
                level,
                anchor,
                text,
            });
        }
        rest = match window.get(text_end..) {
            Some(r) => r,
            None => break,
        };
    }
    toc
}
