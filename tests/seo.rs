//! Wave 2c SEO oracles: front-matter, document head, site artifacts,
//! and the estate-seo floor as executable checks.

#![allow(
    clippy::expect_used,
    clippy::unwrap_used,
    clippy::indexing_slicing,
    clippy::panic
)]

use docs_compiler::{
    compile_site, document, first_h1, first_paragraph, llms_txt, robots_txt, sitemap_xml,
    split_front_matter, PageMeta, SeoPage, SiteConfig,
};
use std::path::Path;

fn write(path: &Path, content: &str) {
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).expect("mkdirs");
    }
    std::fs::write(path, content).expect("write");
}

fn sample_config() -> SiteConfig {
    SiteConfig {
        base_url: "https://docs.example.com".into(),
        site_title: "Estate Docs".into(),
        author: "Wyatt".into(),
        ai_crawlers: "allow".into(),
        stylesheet: Some("/_assets/islands.css".into()),
        island_shim: Some("/_assets/islands/island.mjs".into()),
    }
}

#[test]
fn front_matter_splits_and_falls_back_cleanly() {
    let (meta, body) = split_front_matter(
        "---\ntitle: My Page\ndescription: What it is\nslug: custom-slug\ndate: 2026-11-13\n---\n\n# My Page\n",
    );
    assert_eq!(meta.title.as_deref(), Some("My Page"));
    assert_eq!(meta.description.as_deref(), Some("What it is"));
    assert_eq!(meta.slug.as_deref(), Some("custom-slug"));
    assert_eq!(meta.date.as_deref(), Some("2026-11-13"));
    assert!(
        body.starts_with("# My Page"),
        "front-matter stripped: {body:?}"
    );

    let (meta, body) = split_front_matter("# No front matter\n");
    assert_eq!(meta, PageMeta::default());
    assert_eq!(body, "# No front matter\n");

    let (meta, body) = split_front_matter("---\nunknown: x\ntitle: T\n---\nbody");
    assert_eq!(meta.title.as_deref(), Some("T"), "unknown keys ignored");
    assert_eq!(body, "body");
}

#[test]
fn document_head_carries_the_seo_floor() {
    let meta = PageMeta {
        date: Some("2026-11-13".into()),
        ..PageMeta::default()
    };
    let config = sample_config();
    let doc = document(
        "<h1>Guide</h1><p>Text.</p>",
        &meta,
        "The Guide",
        "What the guide teaches",
        "guides/the-guide.html",
        Some(&config),
    );
    assert!(doc.starts_with("<!doctype html>"), "{doc}");
    assert!(doc.contains("<title>The Guide</title>"), "{doc}");
    assert!(
        doc.contains("<meta name=\"description\" content=\"What the guide teaches\">"),
        "{doc}"
    );
    assert!(
        doc.contains(
            "<link rel=\"canonical\" href=\"https://docs.example.com/guides/the-guide.html\">"
        ),
        "canonical on every page: {doc}"
    );
    assert!(doc.contains("property=\"og:title\""), "{doc}");
    assert!(doc.contains("twitter:card"), "{doc}");
    assert!(
        doc.contains("application/ld+json") && doc.contains("TechArticle"),
        "JSON-LD: {doc}"
    );
    assert!(doc.contains("datePublished\":\"2026-11-13"), "{doc}");
    assert!(
        doc.contains("\"author\":{\"@type\":\"Person\",\"name\":\"Wyatt\"}"),
        "{doc}"
    );
    // Attribute values are escaped.
    let xss = document(
        "<p></p>",
        &PageMeta::default(),
        "T\" onload=\"x",
        "d",
        "a.html",
        None,
    );
    assert!(!xss.contains("onload=\"x\""), "titles are escaped: {xss}");
}

#[test]
fn title_and_description_fall_back_to_content() {
    let body = "<h1>Rendered Heading</h1><p>The first paragraph explains the page in one self-contained sentence.</p>";
    assert_eq!(first_h1(body).as_deref(), Some("Rendered Heading"));
    let description = first_paragraph(body).expect("description");
    assert!(
        description.starts_with("The first paragraph"),
        "{description}"
    );
    assert!(description.len() <= 158, "bounded at ~155: {description}");
}

#[test]
fn site_build_emits_documents_and_all_seo_artifacts() {
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    write(
        &src.join("site.toml"),
        "base_url = \"https://docs.example.com\"\nsite_title = \"Estate Docs\"\nauthor = \"Wyatt\"\nai_crawlers = \"block-training\"\n",
    );
    write(
        &src.join("index.md"),
        "---\ntitle: Home\ndescription: Start here\ndate: 2026-11-13\n---\n\n# Home\n",
    );
    write(
        &src.join("guides/second.md"),
        "# Second guide\n\nIts own intro paragraph for the description.\n",
    );
    // A slug overrides the route stem.
    write(
        &src.join("deep/page.md"),
        "---\nslug: renamed\n---\n\n# Renamed page\n",
    );

    let report = compile_site(&src, &out).expect("walks");
    assert!(report.configured);
    assert_eq!(report.built.len(), 3);
    assert!(report.is_clean(), "{:?}", report.errors);

    // Full documents with canonical + JSON-LD.
    let index = std::fs::read_to_string(out.join("index.html")).expect("index");
    assert!(index.contains("<title>Home</title>"), "{index}");
    assert!(index.contains("rel=\"canonical\""), "{index}");
    assert!(index.contains("application/ld+json"), "{index}");
    assert!(index.contains("<h1>Home</h1>"), "{index}");

    // Description fallback from the first paragraph; slug reroutes.
    let second = std::fs::read_to_string(out.join("guides/second.html")).expect("second");
    assert!(
        second.contains("<meta name=\"description\" content=\"Its own intro paragraph"),
        "first-paragraph fallback: {second}"
    );
    assert!(
        out.join("deep/renamed.html").is_file(),
        "slug reroutes the file"
    );
    assert!(!out.join("deep/page.html").exists(), "the old stem is gone");

    // The GEO floor: llms.txt lists every page with descriptions.
    let llms = std::fs::read_to_string(out.join("llms.txt")).expect("llms.txt");
    assert!(llms.starts_with("# Estate Docs"), "{llms}");
    assert!(
        llms.contains("- [Home](https://docs.example.com/index.html): Start here"),
        "{llms}"
    );
    assert!(llms.contains("guides/second.html"), "{llms}");
    assert!(llms.contains("deep/renamed.html"), "{llms}");

    // Technical floor: sitemap sorted, dated; robots records the policy.
    let sitemap = std::fs::read_to_string(out.join("sitemap.xml")).expect("sitemap");
    assert!(
        sitemap.contains(
            "<loc>https://docs.example.com/index.html</loc><lastmod>2026-11-13</lastmod>"
        ),
        "{sitemap}"
    );
    assert!(sitemap.contains("guides/second.html"), "{sitemap}");
    let robots = std::fs::read_to_string(out.join("robots.txt")).expect("robots");
    assert!(
        robots.contains("AI-crawler policy: block-training"),
        "the decision is recorded: {robots}"
    );
    assert!(robots.contains("User-agent: GPTBot"), "{robots}");
    assert!(
        robots.contains("Sitemap: https://docs.example.com/sitemap.xml"),
        "{robots}"
    );

    // Warnings: the second page has no description... it has one via
    // fallback; the renamed page has neither heading text nor paragraph
    // beyond its h1, so its description fallback is the h1-less path.
    assert!(
        report
            .warnings
            .iter()
            .any(|w| w.contains("deep/renamed.html")),
        "missing-description warning names the route: {:?}",
        report.warnings
    );
}

#[test]
fn robots_policies_and_missing_site_config() {
    let mut config = sample_config();
    config.ai_crawlers = "block-all".into();
    let robots = robots_txt(&config);
    assert!(robots.contains("Disallow: /"), "{robots}");
    assert!(robots.contains("policy: block-all"), "{robots}");

    // No site.toml: pages still become full documents (title +
    // description survive), but no absolute-URL artifacts are emitted.
    let dir = tempfile::tempdir().expect("tempdir");
    let src = dir.path().join("src");
    let out = dir.path().join("out");
    write(&src.join("only.md"), "# Only page\n\nBody text.\n");
    let report = compile_site(&src, &out).expect("walks");
    assert!(!report.configured);
    assert!(!out.join("sitemap.xml").exists(), "no base_url, no sitemap");
    assert!(!out.join("llms.txt").exists());
    let page = std::fs::read_to_string(out.join("only.html")).expect("page");
    assert!(
        page.contains("<title>Only page</title>"),
        "title falls back to the h1: {page}"
    );
    assert!(
        !page.contains("rel=\"canonical\""),
        "no base_url, no canonical: {page}"
    );
    let pages = vec![SeoPage {
        route: "only.html".into(),
        title: "Only page".into(),
        description: "Body text.".into(),
        date: None,
    }];
    let llms = llms_txt(&sample_config(), &pages);
    assert!(
        llms.contains("[Only page](https://docs.example.com/only.html): Body text."),
        "{llms}"
    );
    let sitemap = sitemap_xml(&sample_config(), &pages);
    assert!(
        sitemap.contains("<loc>https://docs.example.com/only.html</loc>"),
        "{sitemap}"
    );
    assert!(
        !sitemap.contains("lastmod"),
        "no date, no lastmod: {sitemap}"
    );
}

#[test]
fn site_config_wires_stylesheet_and_island_shim_into_every_head() {
    let config = sample_config();
    let doc = document(
        "<h1>x</h1>",
        &PageMeta::default(),
        "T",
        "d",
        "t.html",
        Some(&config),
    );
    assert!(
        doc.contains("<link rel=\"stylesheet\" href=\"/_assets/islands.css\">"),
        "{doc}"
    );
    assert!(
        doc.contains("<script type=\"module\" src=\"/_assets/islands/island.mjs\"></script>"),
        "the shim registers <docs-island>: {doc}"
    );
    // No site config: no site-level head wiring.
    let bare = document("<h1>x</h1>", &PageMeta::default(), "T", "d", "t.html", None);
    assert!(
        !bare.contains("stylesheet") && !bare.contains("island.mjs"),
        "{bare}"
    );
}
