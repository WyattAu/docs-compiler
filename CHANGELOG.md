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

## [0.3.1] - 2026-11-13

### Fixed

- **The d2 SVG now survives the markdown pass.** d2's output contains
  blank lines and tab-indented CSS; a blank line ends a raw-HTML block
  in pulldown-cmark, so everything after the first one was re-parsed as
  markdown - the tab-indented CSS turned into indented `<pre><code>`
  blocks and the shapes vanished from the page. The emitted SVG
  collapses blank lines (layout is unaffected). Caught by rendering a
  real page on a phone: the figure was there, the diagram was not.
  0.3.0 is yanked.

## [0.4.0] - 2026-11-13

### Added

- **SEO emission (Wave 2c), per the estate-seo floor:** every site page
  compiles to a FULL HTML document with title, meta description,
  canonical, Open Graph, Twitter card, and JSON-LD (`TechArticle` +
  `WebSite`, named author, `datePublished`); a `site.toml` in the source
  root (`base_url`, `site_title`, `author`, `ai_crawlers`) additionally
  emits `llms.txt` (one curated link per page), `sitemap.xml` (sorted,
  `lastmod` only from declared dates), and `robots.txt` whose
  AI-crawler policy is a RECORDED decision (`allow` default /
  `block-training` / `block-all`, stated in a comment beside the rules).
- Page inputs: `---` front-matter with `title`, `description`, `slug`
  (route override), `date`. Fallbacks: first `<h1>` for title, first
  paragraph (word-bounded at 155 chars) for description, file stem
  otherwise.
- `SiteReport` gains `warnings` (title >60 chars, description >155,
  missing description - the floor's guidance, not errors) and `pages`
  (the SEO view of every built route).
- `serde_json` joins the floor (JSON-LD emission).

## [0.5.0] - 2026-11-14

### Added

- **Precompression (Wave 2c.5):** `compile_site` emits compressed
  sidecars for every artifact - `.gz` (gzip -9), `.br` (brotli -q 11),
  `.zst` (zstd -19) - with tools spawned once per build and absent
  tools degrading to a report warning (spawn-don't-link).
- **Shared-dictionary serving (RFC 9842):** when zstd is present and
  the site has >= 8 artifacts, a 16 KB dictionary is trained across the
  site's own pages (`site.dict`, served with `Use-As-Dictionary:
  match="/*"`), and every html/txt/xml artifact gains a `.dcz` encoding
  decoded against it. Measured on this estate's islands: the smallest
  chunk drops 483 gzip bytes to 264.
- The dev server negotiates `Accept-Encoding` (token presence; q-value
  arithmetic is a documented non-feature): dcz > br > zstd > gz, with
  `Vary: Accept-Encoding` on negotiated responses.
- `SiteReport` gains `artifacts`, `compressed`, `dictionary`.

## [0.5.1] - 2026-11-14

### Added

- `site.toml` gains `stylesheet` and `island_shim`: every page head
  emits the islands' stylesheet link and the `<docs-island>` shim
  module script. The static shell wires its client in config, not
  markup.

## [0.6.0] - 2026-11-15

### Added

- **Site chrome (Wave 2d):** every page wraps in a full layout -
  header with site brand, sidebar nav tree built from the directory
  structure (active page + open group marked), per-page TOC from the
  anchored h2/h3 headings, prev/next pager, and a persisted theme
  toggle (one five-line inline script; everything else dynamic rides
  islands). `chrome::page` performs exact-markup surgery on the
  emitted document - no new templating language.
- **Site assets:** `src/assets/**` mirrors to `out/_assets/**` (the
  islands URL space), and the chrome stylesheet is emitted as
  `/_assets/layout.css` and linked automatically.
- **Search (Wave 2d):** `search.json` (title/url/description/body per
  page, consumed by the solid-fx doc-search island) and `search.bin`
  (a docs-search binary index for the future in-browser reader).
- `site.toml` gains `search_island` (chunk URL for the header search
  trigger island); `SiteConfig` carries `stylesheet`/`island_shim`/
  `search_island` into every page head.
- `HeadingOutline`-style audits get first-class support: heading
  anchor ids now come from docs-core 0.3.0, and `chrome::extract_toc`
  reads the anchored h2/h3 set from any rendered body.

### Changed

- Dependency floor: `docs-search = "0.2"` (the estate's own L0 crate -
  the four capability slices finally meet in one build artifact).

### Fixed (0.6.1)

- Sidebar nav groups grouped by file stem ("guides/one.html" ->
  group "guides/one") instead of directory. Group keys now match the
  page's directory: depth-2 routes under their top segment, deeper
  routes under the first two segments.

### Fixed (0.6.2)

- `docs-core` requirement moved to 0.3: heading anchor ids (0.3.0's
  feature) now reach the chrome TOC. Sidebar nav groups also key on the
  page's directory (depth-2 routes group by top segment; deeper routes
  by the first two).

### Fixed (0.6.3)

- `first_h1` matches `<h1 id="...">` (anchored headings from docs-core
  0.3.0) — the title fallback was silently picking the file stem on
  every anchored page since 0.3.0-shaped bodies appeared.

## [0.7.0] - 2026-11-15

### Added

- **Site chrome module (`chrome.rs`)**: the layout extracted from
  serve-string surgery into a real module — header, sidebar nav tree,
  TOC (from anchored h2/h3), prev/next pager, theme toggle (one
  5-line inline script), search island slot, and the embedded
  `layout.css` (dark default + light palette via `data-theme`,
  container-query header search on narrow screens, emitted as
  `/_assets/layout.css` and linked automatically).
- **Breadcrumbs**: `Home / Guides` rendered per page from the route.
- **`base_path`** (site.toml + `compile_site_with` override +
  `build --base-path`): every site-relative chrome URL follows the
  prefix — subpath hosting (GitHub Pages project previews) works.
- **`og:image`/`twitter:image`** (site.toml `og_image`).
- **`atom.xml`** feed: dated pages, newest first, author injected.
- **Styled chrome-wrapped `404.html`**, served for missing routes.
- **`build` CLI subcommand**: compile a directory without serving,
  with `--base-path` — the production-deploy primitive.
- Dependency floor: `docs-search = "0.2"` (the estate's own L0) — the
  search corpus now also exists as a binary FST index (`search.bin`)
  alongside the JSON corpus, closing the four-slice loop in one build.

### Fixed (0.7.1)

- Site-relative head URLs (stylesheet, island shim) and the search
  corpus URL now honor `base_path` — subpath deploys previously emitted
  root-absolute asset URLs that 404ed under the prefix. External
  (http-s) URLs pass through.

### Fixed (0.7.2)

- `search.json` is now valid JSON: entries are comma-joined instead of
  each carrying a trailing comma — the trailing comma made
  `response.json()` fail in the DocSearch island, so the dropdown never
  showed results. Found by on-device QA ('saga' typed, empty panel).

### Fixed (0.7.3)

- Breadcrumbs: `document()` owns the Home crumb; the site builder passes
  only intermediate directory labels — generated pages no longer render
  "Home / Home".

### Added (0.7.4)

- Site config gains `search_wasm` + `search_bin` (site-relative URLs to
  the docs_search.wasm reader module and the FST binary index). When
  set, the chrome header search island receives all three URLs as
  props — the island queries the wasm reader first and falls back to
  JSON corpus scoring when the module is unavailable. Closing the
  search loop: the FST binary index is now queryable client-side.

## [0.7.5] - 2026-11-20

### Added (0.7.5)

- Site config gains `accent` (`#rgb`/`#rrggbb`): overrides the chrome
  palette's `--accent` for both themes via a cascade-ordered inline
  override (light gets a darkened variant; soft tints derived). Invalid
  hex is ignored and the default palette stands.
