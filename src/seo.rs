//! SEO emission (Wave 2c): full HTML documents with the estate-seo
//! floor's technical artifacts - title, meta description, canonical,
//! Open Graph, JSON-LD per page; `llms.txt`, `sitemap.xml`, and
//! `robots.txt` per site (with `base_url` in `site.toml`).
//!
//! Policy decisions, recorded (estate-seo: never block AI crawlers
//! silently): `site.toml` carries `ai_crawlers = "allow"` (default) /
//! `"block-training"` / `"block-all"`, and the emitted robots.txt states
//! the decision in a comment next to the rules.
//!
//! Inputs are deliberately tiny: front-matter is `key: value` scalars
//! under a leading `---` fence; `site.toml` is `key = "value"` lines.
//! No YAML/TOML dependency; anything richer belongs to a real pipeline.

use std::path::Path;

use crate::directives::html_escape;

/// One indexable page, as the SEO emitters see it.
#[derive(Debug, Clone, PartialEq)]
pub struct SeoPage {
    /// Site-relative HTML route (`index.html`, `guides/x.html`).
    pub route: String,
    /// Page title (front-matter, first h1, or file stem).
    pub title: String,
    /// Meta description (front-matter or first paragraph).
    pub description: String,
    /// Declared publication date (ISO), for sitemap lastmod + JSON-LD.
    pub date: Option<String>,
}

/// Site-wide settings, from `site.toml` in the source root.
#[derive(Debug, Clone, PartialEq)]
pub struct SiteConfig {
    /// Absolute origin for canonical URLs, sitemap, and llms.txt
    /// (`https://docs.example.com`, no trailing slash).
    pub base_url: String,
    /// Site title (llms.txt heading, JSON-LD `isPartOf`).
    pub site_title: String,
    /// Named author (E-E-A-T: real names beat anonymous).
    pub author: String,
    /// AI-crawler policy, recorded in robots.txt: `"allow"` (default),
    /// `"block-training"`, `"block-all"`.
    pub ai_crawlers: String,
    /// Stylesheet URL emitted as a `<link rel="stylesheet">` in every
    /// page head (e.g. the islands' Tailwind build).
    pub stylesheet: Option<String>,
    /// The island shim module URL, emitted as a deferred module script
    /// in every page head (registers `<docs-island>`).
    pub island_shim: Option<String>,
}

impl SiteConfig {
    /// Parse `site.toml` if present: `key = "value"` scalar lines only.
    /// `base_url` is the one required key; the rest have defaults.
    pub fn load(src: &Path) -> Option<SiteConfig> {
        let text = std::fs::read_to_string(src.join("site.toml")).ok()?;
        let mut base_url = None;
        let mut site_title = String::from("Documentation");
        let mut author = String::new();
        let mut ai_crawlers = String::from("allow");
        let mut stylesheet = None;
        let mut island_shim = None;
        for line in text.lines() {
            let line = line.trim();
            if line.is_empty() || line.starts_with('#') {
                continue;
            }
            let Some((key, value)) = line.split_once('=') else {
                continue;
            };
            let value = value.trim().trim_matches('"').to_string();
            match key.trim() {
                "base_url" => base_url = Some(value.trim_end_matches('/').to_string()),
                "site_title" => site_title = value,
                "author" => author = value,
                "ai_crawlers" => ai_crawlers = value,
                "stylesheet" => stylesheet = Some(value),
                "island_shim" => island_shim = Some(value),
                _ => {}
            }
        }
        Some(SiteConfig {
            base_url: base_url.map(|u| u.trim_end_matches('/').to_string())?,
            site_title,
            author,
            ai_crawlers,
            stylesheet,
            island_shim,
        })
    }
}

/// Per-page metadata, from the page's front-matter (`---` fenced
/// `key: value` scalars). `title` falls back to the first `<h1>`, then
/// the file stem; `description` falls back to the first paragraph.
#[derive(Debug, Clone, Default, PartialEq)]
pub struct PageMeta {
    /// Page title.
    pub title: Option<String>,
    /// Meta description.
    pub description: Option<String>,
    /// Output file stem override (route becomes `<slug>.html`).
    pub slug: Option<String>,
    /// Publication date (ISO `YYYY-MM-DD`; sitemap lastmod + JSON-LD).
    pub date: Option<String>,
}

/// Split a leading `---` front-matter block off a document: returns the
/// parsed metadata and the remainder to compile.
#[must_use]
pub fn split_front_matter(markdown: &str) -> (PageMeta, String) {
    let mut meta = PageMeta::default();
    let Some(rest) = markdown.strip_prefix("---\n") else {
        return (meta, markdown.to_string());
    };
    let Some(close) = rest.find("\n---") else {
        return (meta, markdown.to_string());
    };
    let block = &rest[..close];
    for line in block.lines() {
        let Some((key, value)) = line.split_once(':') else {
            continue;
        };
        let value = value.trim().to_string();
        match key.trim() {
            "title" => meta.title = Some(value),
            "description" => meta.description = Some(value),
            "slug" => meta.slug = Some(value),
            "date" => meta.date = Some(value),
            _ => {}
        }
    }
    // Skip the closing fence line and any following blank lines
    // (markdown leading whitespace is insignificant).
    let after = &rest[close + 4..];
    (meta, after.trim_start_matches('\n').to_string())
}

/// Derive a page title from the rendered body: the first `<h1>` text.
#[must_use]
pub fn first_h1(body: &str) -> Option<String> {
    let start = body.find("<h1>")? + 4;
    let end = body[start..].find("</h1>")? + start;
    let text = strip_tags(&body[start..end]);
    if text.is_empty() {
        None
    } else {
        Some(text)
    }
}

/// Derive a description from the rendered body: the first paragraph's
/// text, tags stripped, truncated at a word boundary to 155 chars.
#[must_use]
pub fn first_paragraph(body: &str) -> Option<String> {
    let start = body.find("<p>")? + 3;
    let end = body[start..].find("</p>")? + start;
    let text = strip_tags(&body[start..end]);
    if text.is_empty() {
        return None;
    }
    Some(truncate_words(&text, 155))
}

fn strip_tags(html: &str) -> String {
    let mut out = String::with_capacity(html.len());
    let mut in_tag = false;
    for c in html.chars() {
        match c {
            '<' => in_tag = true,
            '>' => in_tag = false,
            c if !in_tag => out.push(c),
            _ => {}
        }
    }
    out.trim().to_string()
}

fn truncate_words(text: &str, max: usize) -> String {
    if text.len() <= max {
        return text.to_string();
    }
    match text[..max].rfind(' ') {
        Some(pos) => format!("{}...", &text[..pos]),
        None => format!("{}...", &text[..max]),
    }
}

/// Attribute-safe escaping: text escaping plus double quotes (SEO head
/// values live inside `content="..."` attributes).
fn escape_attr(text: &str) -> String {
    html_escape(text).replace('"', "&quot;")
}

/// The full-document wrapper: doctype, head (charset, viewport, title,
/// description, canonical, Open Graph, Twitter card, JSON-LD), body.
/// `route` is the page's site-relative HTML path (`index.html`,
/// `guides/x.html`). Length guidance (title <=60, description <=155)
/// surfaces as warnings in the report, not build failures.
#[must_use]
pub fn document(
    body: &str,
    meta: &PageMeta,
    title: &str,
    description: &str,
    route: &str,
    config: Option<&SiteConfig>,
) -> String {
    let canonical = config
        .map(|c| format!("{}/{}", c.base_url, route))
        .unwrap_or_default();
    let canonical_tags = if canonical.is_empty() {
        String::new()
    } else {
        format!(
            "<link rel=\"canonical\" href=\"{u}\"><meta property=\"og:url\" content=\"{u}\">",
            u = escape_attr(&canonical)
        )
    };
    let json_ld_tag = config
        .map(|c| {
            let mut article = serde_json::json!({
                "@context": "https://schema.org",
                "@type": "TechArticle",
                "headline": title,
                "description": description,
                "isPartOf": { "@type": "WebSite", "name": c.site_title, "url": c.base_url },
            });
            if let Some(obj) = article.as_object_mut() {
                if !canonical.is_empty() {
                    obj.insert("url".into(), serde_json::json!(canonical));
                }
                if !c.author.is_empty() {
                    obj.insert(
                        "author".into(),
                        serde_json::json!({ "@type": "Person", "name": c.author }),
                    );
                }
                if let Some(date) = &meta.date {
                    obj.insert("datePublished".into(), serde_json::json!(date));
                }
            }
            format!("<script type=\"application/ld+json\">{article}</script>")
        })
        .unwrap_or_default();
    let site_name_tag = config
        .map(|c| {
            format!(
                "<meta property=\"og:site_name\" content=\"{}\">",
                escape_attr(&c.site_title)
            )
        })
        .unwrap_or_default();
    // Site-level head wiring (Wave 2c.5): the islands' stylesheet and
    // the shim module that registers <docs-island>.
    let head_extra = config
        .map(|c| {
            let mut extra = String::new();
            if let Some(css) = &c.stylesheet {
                extra.push_str(&format!(
                    "<link rel=\"stylesheet\" href=\"{}\">",
                    escape_attr(css)
                ));
            }
            if let Some(shim) = &c.island_shim {
                extra.push_str(&format!(
                    "<script type=\"module\" src=\"{}\"></script>",
                    escape_attr(shim)
                ));
            }
            extra
        })
        .unwrap_or_default();
    format!(
        "<!doctype html><html lang=\"en\"><head><meta charset=\"utf-8\">\
<meta name=\"viewport\" content=\"width=device-width, initial-scale=1\">\
<title>{t}</title>\
<meta name=\"description\" content=\"{d}\">{canonical_tags}\
<meta property=\"og:type\" content=\"article\">\
<meta property=\"og:title\" content=\"{t}\">\
<meta property=\"og:description\" content=\"{d}\">{site_name_tag}\
<meta name=\"twitter:card\" content=\"summary\">\
<meta name=\"twitter:title\" content=\"{t}\">\
<meta name=\"twitter:description\" content=\"{d}\">\
{head_extra}{json_ld_tag}</head><body>{body}</body></html>",
        t = escape_attr(title),
        d = escape_attr(description)
    )
}

/// `llms.txt` (GEO floor): the site heading plus one curated link per
/// page with its description.
#[must_use]
pub fn llms_txt(config: &SiteConfig, pages: &[SeoPage]) -> String {
    let mut out = format!("# {}\n\n", config.site_title);
    if !config.author.is_empty() {
        out.push_str(&format!("By {}.\n\n", config.author));
    }
    for page in pages {
        out.push_str(&format!(
            "- [{t}]({u}/{r}): {d}\n",
            t = page.title,
            u = config.base_url,
            r = page.route,
            d = page.description
        ));
    }
    out
}

/// `sitemap.xml`: one `<url>` per page, sorted by route; `lastmod` only
/// when the page declares a date (deterministic builds do not lie about
/// modification times).
#[must_use]
pub fn sitemap_xml(config: &SiteConfig, pages: &[SeoPage]) -> String {
    let mut out = String::from(
        "<?xml version=\"1.0\" encoding=\"UTF-8\"?>\n<urlset xmlns=\"http://www.sitemaps.org/schemas/sitemap/0.9\">\n",
    );
    let mut sorted: Vec<&SeoPage> = pages.iter().collect();
    sorted.sort_by(|a, b| a.route.cmp(&b.route));
    for page in sorted {
        let lastmod = page
            .date
            .as_ref()
            .map(|d| format!("<lastmod>{d}</lastmod>"))
            .unwrap_or_default();
        out.push_str(&format!(
            "<url><loc>{}/{}</loc>{lastmod}</url>\n",
            config.base_url, page.route
        ));
    }
    out.push_str("</urlset>\n");
    out
}

/// `robots.txt` with the AI-crawler policy as a recorded decision.
#[must_use]
pub fn robots_txt(config: &SiteConfig) -> String {
    let policy = config.ai_crawlers.as_str();
    let mut out =
        format!("# AI-crawler policy: {policy} (recorded decision; site.toml: ai_crawlers)\n");
    match policy {
        "block-all" => {
            out.push_str("User-agent: *\nDisallow: /\n");
        }
        "block-training" => {
            for bot in ["GPTBot", "CCBot", "anthropic-ai"] {
                out.push_str(&format!("User-agent: {bot}\nDisallow: /\n"));
            }
            out.push_str("User-agent: *\nAllow: /\n");
        }
        _ => {
            out.push_str("User-agent: *\nAllow: /\n");
        }
    }
    out.push_str(&format!("Sitemap: {}/sitemap.xml\n", config.base_url));
    out
}
