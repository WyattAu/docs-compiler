//! The CLI: compile one markdown document to one HTML page, or run the
//! dev server over a directory (Wave 2).

use clap::Parser;
use notify::Watcher as _;
use std::io::Write as _;
use std::path::{Path, PathBuf};
use std::sync::mpsc;
use std::time::{Duration, Instant};

#[derive(Debug, Parser)]
#[command(
    name = "docs-compiler",
    about = "Compile estate markdown with directive fences to static HTML",
    version
)]
struct Cli {
    #[command(subcommand)]
    command: Command,
}

#[derive(Debug, clap::Subcommand)]
enum Command {
    /// Compile one markdown document to one HTML page.
    Compile {
        /// Input markdown file.
        input: PathBuf,
        /// Output HTML file (default: the input path with a .html suffix).
        #[arg(short, long)]
        output: Option<PathBuf>,
        /// Root directory for directive-relative paths
        /// (default: the input file's directory).
        #[arg(short, long)]
        root: Option<PathBuf>,
    },
    /// Compile a directory of markdown into HTML and serve it,
    /// recompiling when any source file changes (the dev server).
    Serve {
        /// Directory of markdown sources (recursive; hidden dirs skipped).
        src: PathBuf,
        /// Output directory for the HTML (default: `<src>/../_site`).
        #[arg(long)]
        out: Option<PathBuf>,
        /// Listen port (default 8080).
        #[arg(long, default_value_t = 8080)]
        port: u16,
        /// Bind address (default: all interfaces, so phones on the LAN
        /// can open the preview).
        #[arg(long, default_value = "0.0.0.0")]
        addr: String,
    },
}

fn main() {
    match Cli::parse().command {
        Command::Compile {
            input,
            output,
            root,
        } => compile_one(&input, output, root),
        Command::Serve {
            src,
            out,
            port,
            addr,
        } => serve(&src, out, port, &addr),
    }
}

fn compile_one(input: &Path, output: Option<PathBuf>, root: Option<PathBuf>) {
    let root = root.unwrap_or_else(|| {
        input
            .parent()
            .filter(|p| !p.as_os_str().is_empty())
            .map_or_else(|| PathBuf::from("."), Path::to_path_buf)
    });
    let source = match std::fs::read_to_string(input) {
        Ok(s) => s,
        Err(e) => {
            eprintln!("cannot read {}: {e}", input.display());
            std::process::exit(2);
        }
    };
    match docs_compiler::compile(&source, &docs_compiler::BuildContext::new(root)) {
        Ok(html) => {
            let output = output.unwrap_or_else(|| {
                let mut p = input.as_os_str().to_owned();
                p.push(".html");
                PathBuf::from(p)
            });
            if let Err(e) = std::fs::write(&output, html) {
                eprintln!("cannot write {}: {e}", output.display());
                std::process::exit(2);
            }
        }
        Err(e) => {
            eprintln!("build failed: {e}");
            std::process::exit(1);
        }
    }
}

/// The dev server: one pass to build, then serve + watch. Recompile on
/// any source event, debounced; a bad page reports and keeps serving the
/// last good HTML.
fn serve(src: &Path, out: Option<PathBuf>, port: u16, addr: &str) {
    let out = out.unwrap_or_else(|| src.parent().unwrap_or(Path::new(".")).join("_site"));
    let report = match docs_compiler::compile_site(src, &out) {
        Ok(r) => r,
        Err(e) => {
            eprintln!("cannot build site {}: {e}", src.display());
            std::process::exit(2);
        }
    };
    print_report(&report);
    if !report.is_clean() {
        eprintln!("(serving anyway: the good pages are up, fix the rest and save)");
    }

    let (tx, rx) = mpsc::channel();
    let watcher = notify::recommended_watcher(move |res: notify::Result<notify::Event>| {
        if res.is_ok() {
            let _ = tx.send(());
        }
    })
    .map_or_else(
        |e| {
            eprintln!("cannot watch {}: {e}", src.display());
            std::process::exit(2);
        },
        |mut w| {
            if let Err(e) = w.watch(src, notify::RecursiveMode::Recursive) {
                eprintln!("cannot watch {}: {e}", src.display());
                std::process::exit(2);
            }
            w
        },
    );
    // Keep the watcher handle alive for the process lifetime.
    std::mem::forget(watcher);

    let server = tiny_http::Server::http(format!("{addr}:{port}")).unwrap_or_else(|e| {
        eprintln!("cannot listen on {addr}:{port}: {e}");
        std::process::exit(2);
    });
    println!("serving {} at http://{addr}:{port}", out.display());
    if let Some(ip) = lan_ip() {
        println!("from other devices: http://{ip}:{port}");
    }

    let debounce = Duration::from_millis(400);
    let mut last_rebuild = Instant::now();
    loop {
        // Serve everything queued, then check for source changes.
        while let Ok(Some(request)) = server.recv_timeout(Duration::from_millis(200)) {
            respond(request, &out);
        }
        let mut dirty = false;
        while rx.try_recv().is_ok() {
            dirty = true;
        }
        if dirty && last_rebuild.elapsed() >= debounce {
            let fresh = docs_compiler::compile_site(src, &out);
            match fresh {
                Ok(report) => {
                    print_report(&report);
                    last_rebuild = Instant::now();
                }
                Err(e) => eprintln!("rebuild failed: {e}"),
            }
        }
    }
}

fn print_report(report: &docs_compiler::SiteReport) {
    println!(
        "built {} page(s), {} compressed sidecar(s){}",
        report.built.len(),
        report.compressed,
        if report.dictionary {
            ", shared dictionary: site.dict"
        } else {
            ""
        }
    );
    for (path, error) in &report.errors {
        eprintln!("  FAILED {}: {error}", path.display());
    }
    for warning in &report.warnings {
        eprintln!("  note: {warning}");
    }
}

fn respond(request: tiny_http::Request, out: &Path) {
    let url = request.url().to_owned();
    let accept = request
        .headers()
        .iter()
        .find(|h| h.field.equiv("Accept-Encoding"))
        .map(|h| h.value.as_str().to_owned())
        .unwrap_or_default();
    let header =
        |field: &'static str, value: &'static str| tiny_http::Header::from_bytes(field, value).ok();
    let attach = |request: tiny_http::Request,
                  mut response: tiny_http::Response<std::io::Cursor<Vec<u8>>>,
                  mime: &'static str,
                  encoding: Option<&'static str>|
     -> std::io::Result<()> {
        if let Some(token) = encoding {
            if let Some(h) = header("Content-Encoding", token) {
                response = response.with_header(h);
            }
            // Negotiated responses vary by the client's encodings.
            if let Some(h) = header("Vary", "Accept-Encoding") {
                response = response.with_header(h);
            }
        }
        if let Some(h) = header("Content-Type", mime) {
            response = response.with_header(h);
        }
        request.respond(response)
    };

    // The shared dictionary rides `Use-As-Dictionary` (RFC 9842): the
    // client caches it once, then `dcz` responses decode against it.
    if url.split(['?', '#']).next() == Some("/site.dict") {
        let path = out.join("site.dict");
        if path.is_file() {
            match std::fs::read(&path) {
                Ok(data) => {
                    let mut response = tiny_http::Response::from_data(data);
                    if let Some(h) = header("Content-Type", "application/zstd") {
                        response = response.with_header(h);
                    }
                    if let Some(h) = header("Use-As-Dictionary", "match=\"/*\"") {
                        response = response.with_header(h);
                    }
                    let _ = request.respond(response);
                    return;
                }
                Err(e) => {
                    let _ = writeln!(std::io::stderr(), "cannot read site.dict: {e}");
                }
            }
        }
    }

    match docs_compiler::resolve_route(out, &url) {
        Some(path) => {
            let encoding = docs_compiler::pick_encoding(&accept, &path);
            let source = encoding
                .as_ref()
                .map(|(_, p)| p)
                .unwrap_or(&path)
                .to_path_buf();
            let mime = docs_compiler::mime_for(&path);
            match std::fs::read(&source) {
                Ok(data) => {
                    let response = tiny_http::Response::from_data(data).with_status_code(200);
                    let _ = attach(request, response, mime, encoding.map(|(t, _)| t));
                }
                Err(e) => {
                    let _ = writeln!(std::io::stderr(), "cannot read {}: {e}", path.display());
                    let response = tiny_http::Response::from_data(b"internal error".to_vec())
                        .with_status_code(500);
                    let _ = attach(request, response, "text/plain; charset=utf-8", None);
                }
            }
        }
        None => {
            let response =
                tiny_http::Response::from_data(b"not found".to_vec()).with_status_code(404);
            let _ = attach(request, response, "text/plain; charset=utf-8", None);
        }
    }
}

/// Best-effort LAN IP for the "from other devices" hint (a phone on the
/// same network opens the preview by this address).
fn lan_ip() -> Option<String> {
    let output = std::process::Command::new("sh")
        .args([
            "-c",
            "ip -4 addr show scope global | grep -oP '(?<=inet\\s)\\d+(\\.\\d+){3}' | head -1",
        ])
        .output()
        .ok()?;
    String::from_utf8(output.stdout)
        .ok()
        .map(|s| s.trim().to_owned())
        .filter(|s| !s.is_empty())
}
