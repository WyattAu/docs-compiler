//! Build-time precompression (Wave 2c.5): emit compressed sidecars for
//! every site artifact and train a shared zstd dictionary across the
//! site's own pages. Measured on this estate's payload (218 near-
//! identical island chunks): dictionary compression took the smallest
//! chunk from 483 gz bytes to 264 - gzip's DEFLATE ceiling cannot reach
//! that, and forking a compressor cannot either (the format is the
//! limit, not the code).
//!
//! Doctrine: tools are spawned, never linked (REQ-P1 style). `gzip`,
//! `brotli`, and `zstd` are detected once per build; absent tools skip
//! their variants with a report warning instead of failing the build.
//! The shared dictionary follows RFC 9842 (`Content-Encoding: dcz`):
//! the server sends `/site.dict` with `Use-As-Dictionary`, Chromium
//! caches it, and subsequent `.dcz` responses decode against it.

use std::io;
use std::path::{Path, PathBuf};
use std::process::{Command, Stdio};

/// Which compressors exist on this host.
#[derive(Debug, Clone, Copy, Default)]
pub struct Tools {
    /// `gzip` present.
    pub gzip: bool,
    /// `brotli` present.
    pub brotli: bool,
    /// `zstd` present.
    pub zstd: bool,
}

impl Tools {
    /// Detect once per build (three `--version` spawns).
    #[must_use]
    pub fn detect() -> Self {
        Self {
            gzip: available("gzip", &["--version"]),
            brotli: available("brotli", &["--version"]),
            zstd: available("zstd", &["--version"]),
        }
    }

    /// Whether any compression variant can be produced.
    #[must_use]
    pub fn any(self) -> bool {
        self.gzip || self.brotli || self.zstd
    }
}

fn available(tool: &str, args: &[&str]) -> bool {
    Command::new(tool)
        .args(args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()
        .is_ok_and(|s| s.success())
}

/// Emit compressed sidecars for one artifact: `path` → `path.gz`,
/// `path.br`, `path.zst` (whatever the tools allow). Returns the
/// produced `Content-Encoding` tokens.
pub fn sidecars(tools: &Tools, path: &Path) -> io::Result<Vec<&'static str>> {
    let mut produced = Vec::new();
    let jobs: [(&str, &str, &[&str]); 3] = [
        ("gzip", "gz", &["-9", "-c"]),
        ("brotli", "br", &["-q", "11", "-c"]),
        ("zstd", "zst", &["-19", "-q", "-c"]),
    ];
    for (tool, ext, args) in jobs {
        let allowed = match tool {
            "gzip" => tools.gzip,
            "brotli" => tools.brotli,
            _ => tools.zstd,
        };
        if !allowed || spawn_to_file(tool, args, path, &sidecar_path(path, ext))? {
            produced.push(ext_token(ext));
        }
    }
    Ok(produced)
}

/// The sidecar path for one encoding (`x.html` → `x.html.gz`).
#[must_use]
pub fn sidecar_path(path: &Path, ext: &str) -> PathBuf {
    let mut name = path.as_os_str().to_owned();
    name.push(".");
    name.push(ext);
    PathBuf::from(name)
}

/// The `Content-Encoding` token for a sidecar extension.
#[must_use]
pub fn ext_token(ext: &str) -> &'static str {
    match ext {
        "gz" => "gzip",
        "br" => "br",
        "zst" => "zstd",
        "dcz" => "dcz",
        _ => "identity",
    }
}

/// Spawn `tool args input`, writing stdout to `dest`. Returns whether
/// the output file was produced non-empty (some tools emit nothing on
/// empty input).
fn spawn_to_file(tool: &str, args: &[&str], input: &Path, dest: &Path) -> io::Result<bool> {
    let out_file = std::fs::File::create(dest)?;
    let status = Command::new(tool)
        .args(args)
        .arg(input)
        .stdin(Stdio::null())
        .stdout(Stdio::from(out_file))
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        let _ = std::fs::remove_file(dest);
        return Ok(false);
    }
    Ok(std::fs::metadata(dest)
        .map(|m| m.len() > 0)
        .unwrap_or(false))
}

/// Train a shared zstd dictionary across `samples` (the site's own
/// pages), writing it to `dest`. Returns false when there are too few
/// samples (zstd needs several to train) or zstd is absent - callers
/// then skip `.dcz` emission with a warning.
pub fn train_dictionary(tools: &Tools, samples: &[PathBuf], dest: &Path) -> io::Result<bool> {
    if !tools.zstd || samples.len() < 8 {
        return Ok(false);
    }
    let mut args: Vec<&str> = vec!["--train", "--maxdict=16384"];
    let strs: Vec<String> = samples
        .iter()
        .map(|p| p.to_string_lossy().into_owned())
        .collect();
    for s in &strs {
        args.push(s);
    }
    args.push("-o");
    let dest_str = dest.to_string_lossy().into_owned();
    args.push(&dest_str);
    let status = Command::new("zstd")
        .args(&args)
        .stdin(Stdio::null())
        .stdout(Stdio::null())
        .stderr(Stdio::null())
        .status()?;
    Ok(status.success() && dest.is_file())
}

/// Emit one `.dcz` sidecar (zstd compressed WITH the shared dictionary)
/// for a page. Only meaningful when a dictionary was trained.
pub fn dcz_sidecar(tools: &Tools, dict: &Path, path: &Path) -> io::Result<bool> {
    if !tools.zstd || !dict.is_file() {
        return Ok(false);
    }
    let mut dest = path.as_os_str().to_owned();
    dest.push(".dcz");
    let status = Command::new("zstd")
        .arg("-19")
        .arg("-D")
        .arg(dict)
        .arg("-c")
        .arg(path)
        .stdin(Stdio::null())
        .stdout(Stdio::from(std::fs::File::create(&dest)?))
        .stderr(Stdio::null())
        .status()?;
    if !status.success() {
        let _ = std::fs::remove_file(dest);
        return Ok(false);
    }
    Ok(true)
}

/// Pick the best encoding for a request: reads the client's
/// `Accept-Encoding` (token presence only, no q-value arithmetic -
/// documented limitation) against the sidecars that exist for `path`.
/// Preference: dcz (dictionary-compressed) > br > zstd > gz.
#[must_use]
pub fn pick_encoding(accept: &str, path: &Path) -> Option<(&'static str, PathBuf)> {
    let accepted = |token: &str| {
        accept.split(',').any(|part| {
            part.split(';')
                .next()
                .unwrap_or("")
                .trim()
                .eq_ignore_ascii_case(token)
        })
    };
    let candidates: [(&str, &str); 4] = [
        ("dcz", "dcz"),
        ("br", "br"),
        ("zstd", "zst"),
        ("gzip", "gz"),
    ];
    for (token, ext) in candidates {
        if accepted(token) && sidecar_path(path, ext).is_file() {
            return Some((ext_token(ext), sidecar_path(path, ext)));
        }
    }
    None
}
