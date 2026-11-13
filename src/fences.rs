//! Fence scanning: find directive fences in markdown, parse attributes.

/// The four directive fences this compiler resolves. Island fences are
/// not here: docs-core renders those natively.
#[derive(Debug, Clone, PartialEq)]
pub enum DirectiveKind {
    /// `include-symbol file=... symbol=...`
    IncludeSymbol,
    /// `witness wasm=... [fuel=...] [assert_exit=...] [assert_stdout=...]`
    Witness,
    /// `lean-check file=... theorem=... toolchain=...`
    LeanCheck,
    /// `d2` + body: a D2 diagram rendered to static SVG at build time.
    Diagram,
}

impl DirectiveKind {
    fn label(&self) -> &'static str {
        match self {
            Self::IncludeSymbol => "include-symbol",
            Self::Witness => "witness",
            Self::LeanCheck => "lean-check",
            Self::Diagram => "d2",
        }
    }

    fn from_info(info: &str) -> Option<Self> {
        if info.starts_with("include-symbol") {
            Some(Self::IncludeSymbol)
        } else if info.starts_with("witness") {
            Some(Self::Witness)
        } else if info.starts_with("lean-check") {
            Some(Self::LeanCheck)
        } else if info.starts_with("d2") {
            Some(Self::Diagram)
        } else {
            None
        }
    }
}

/// One resolved directive fence.
#[derive(Debug, Clone, PartialEq)]
pub struct Directive {
    /// Which fence this is.
    pub kind: DirectiveKind,
    /// Parsed `key=value` attributes (quotes stripped, order preserved).
    pub attrs: Vec<(String, String)>,
    /// The fence body: everything between the opening and closing fence
    /// lines, verbatim. (File-reading directives ignore it; body-driven
    /// directives like `d2` own it.)
    pub body: String,
    /// Byte range of the whole fence (opening line through closing
    /// triple-backtick), into the original source.
    pub byte_range: (usize, usize),
}

impl Directive {
    /// The value of an attribute, by key.
    pub fn attr(&self, key: &str) -> Option<&str> {
        self.attrs
            .iter()
            .find(|(k, _)| k == key)
            .map(|(_, v)| v.as_str())
    }

    /// The value of a required attribute, or a typed failure reason.
    pub fn require(&self, key: &str) -> Result<&str, crate::CompilerError> {
        self.attr(key)
            .ok_or_else(|| crate::CompilerError::Attribute {
                directive: self.kind.label(),
                reason: format!("missing required attribute `{key}`"),
            })
    }
}

/// A scanned source: raw markdown segments and directive fences, in
/// document order.
#[derive(Debug)]
pub struct Scanned<'a> {
    source: &'a str,
    blocks: Vec<Block>,
}

/// A scanned source block: raw markdown text or a directive fence.
#[derive(Debug)]
pub enum Block {
    /// Raw markdown text, byte range into the source.
    Text(usize, usize),
    /// A resolved-ready directive fence.
    Directive(Directive),
}

impl<'a> Scanned<'a> {
    /// All blocks in document order.
    pub fn blocks(&self) -> &[Block] {
        &self.blocks
    }

    /// The original source.
    pub fn source(&self) -> &'a str {
        self.source
    }

    /// Byte range of a block.
    pub fn range(block: &Block) -> (usize, usize) {
        match block {
            Block::Text(start, end) => (*start, *end),
            Block::Directive(d) => d.byte_range,
        }
    }

    /// Rebuild the source with every directive fence replaced by the
    /// output of `resolve` (raw HTML blocks; docs-core passes raw HTML
    /// through unchanged).
    pub fn substitute<F>(&self, mut resolve: F) -> Result<String, crate::CompilerError>
    where
        F: FnMut(&Directive) -> Result<String, crate::CompilerError>,
    {
        let mut out = String::with_capacity(self.source.len());
        let mut cursor = 0usize;
        for block in &self.blocks {
            let (start, end) = Self::range(block);
            out.push_str(self.source.get(cursor..start).unwrap_or_default());
            match block {
                Block::Text(..) => {
                    out.push_str(self.source.get(start..end).unwrap_or_default());
                }
                Block::Directive(d) => {
                    out.push_str(&resolve(d)?);
                    out.push_str("\n\n");
                }
            }
            cursor = end;
        }
        out.push_str(self.source.get(cursor..).unwrap_or_default());
        Ok(out)
    }
}

/// Scan markdown for directive fences. Unrecognized fences (plain code,
/// islands) are left as raw text for docs-core.
pub fn scan(source: &str) -> Scanned<'_> {
    let mut blocks: Vec<Block> = Vec::new();
    let bytes = source.as_bytes();
    let mut cursor = 0usize;
    let mut text_start = 0usize;

    while cursor < bytes.len() {
        let line_end = source[cursor..]
            .find('\n')
            .map(|p| cursor + p)
            .unwrap_or(bytes.len());
        let line = source.get(cursor..line_end).unwrap_or_default();
        let trimmed = line.trim_start();
        if let Some(rest) = trimmed.strip_prefix("```") {
            let info = rest.trim();
            // CommonMark: a fence's info string cannot contain backticks;
            // a line like ```lang x``` is not a valid opening fence, and
            // treating it as one would swallow the rest of the document.
            if let Some(kind) = DirectiveKind::from_info(info).filter(|_| !info.contains('`')) {
                // Flush pending text up to the fence opening.
                if cursor > text_start {
                    blocks.push(Block::Text(text_start, cursor));
                }
                // Find the closing fence line.
                let mut close = None;
                let mut scan = line_end + 1;
                while scan <= bytes.len() {
                    let e = source[scan..]
                        .find('\n')
                        .map(|p| scan + p)
                        .unwrap_or(bytes.len());
                    if source.get(scan..e).unwrap_or_default().trim() == "```" {
                        close = Some((scan, e));
                        break;
                    }
                    scan = e + 1;
                }
                let (close_start, close_end) = close.unwrap_or((bytes.len(), bytes.len()));
                // The body is everything between the opening line and the
                // closing fence line, verbatim.
                let body = source
                    .get(line_end + 1..close_start)
                    .unwrap_or_default()
                    .to_string();
                blocks.push(Block::Directive(Directive {
                    kind,
                    attrs: parse_attrs(info),
                    body,
                    byte_range: (cursor, close_end),
                }));
                cursor = close_end;
                text_start = close_end;
                continue;
            }
        }
        cursor = line_end + 1;
    }
    if text_start < bytes.len() {
        blocks.push(Block::Text(text_start, bytes.len()));
    }
    Scanned { source, blocks }
}

/// Parse `key=value` / `key="value with spaces"` tokens from a fence
/// info string, after the directive word. Order preserved, duplicates
/// keep their first value.
fn parse_attrs(info: &str) -> Vec<(String, String)> {
    let mut attrs = Vec::new();
    let rest = match info.find(char::is_whitespace) {
        Some(pos) => &info[pos..],
        None => "",
    };
    let mut tokens = rest.split_whitespace();
    while let Some(token) = tokens.next() {
        let Some(eq) = token.find('=') else {
            continue;
        };
        let Some(key) = token.get(..eq) else {
            continue;
        };
        let Some(mut value) = token.get(eq + 1..).map(str::to_string) else {
            continue;
        };
        if value.starts_with('"') && !value.ends_with('"') {
            // Quoted value with spaces: keep consuming tokens.
            for cont in tokens.by_ref() {
                value.push(' ');
                value.push_str(cont);
                if cont.ends_with('"') {
                    break;
                }
            }
        }
        let value = value.trim_matches('"').to_string();
        if !attrs.iter().any(|(k, _)| k == key) {
            attrs.push((key.to_string(), value));
        }
    }
    attrs
}

#[cfg(test)]
// Test-only lifts (docs-core decoder-pattern precedent): assertions index
// on purpose; the library paths keep the denies.
#[allow(clippy::indexing_slicing)]
mod tests {
    use super::*;

    #[test]
    fn finds_directive_fences_and_keeps_other_fences() {
        let md = "intro\n\n```include-symbol file=\"a.rs\" symbol=\"x\"\ncode here\n```\n\n\
                  ```rust\nfn plain() {}\n```\n\n```witness wasm=\"m.wasm\" fuel=1000\nbody\n```\n";
        let scanned = scan(md);
        let directives: Vec<&Directive> = scanned
            .blocks()
            .iter()
            .filter_map(|b| match b {
                Block::Directive(d) => Some(d),
                _ => None,
            })
            .collect();
        assert_eq!(directives.len(), 2);
        assert_eq!(directives[0].kind, DirectiveKind::IncludeSymbol);
        assert_eq!(
            directives[0].attr("file"),
            Some("a.rs"),
            "quoted value unquoted"
        );
        assert_eq!(directives[1].kind, DirectiveKind::Witness);
        assert_eq!(directives[1].attr("fuel"), Some("1000"));
    }

    #[test]
    fn quoted_values_with_spaces_stay_one_value() {
        let attrs = parse_attrs("witness assert_stdout=\"hello world\" fuel=5");
        assert_eq!(
            attrs,
            vec![
                ("assert_stdout".to_string(), "hello world".to_string()),
                ("fuel".to_string(), "5".to_string()),
            ]
        );
    }
}
