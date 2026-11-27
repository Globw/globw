//! The "ui" module's helpers: open a URL in the browser, so a Globw program can
//! serve a web interface (HTML/CSS/JS) and show it, plus the static-file logic
//! shared by `ui.serve` and `serve.file`. The page is the UI, Globw is the logic.

use std::process::Command;

/// Opens `url` in the user's default browser.
pub(super) fn ui_open(url: &str) -> Result<(), String> {
    let spawned = if cfg!(windows) {
        // `start` needs an empty "" as the window title before the URL
        Command::new("cmd").args(["/C", "start", "", url]).spawn()
    } else if cfg!(target_os = "macos") {
        Command::new("open").arg(url).spawn()
    } else {
        Command::new("xdg-open").arg(url).spawn()
    };
    spawned.map(|_| ()).map_err(|e| format!("ui.open: cannot open the browser ({})", e))
}

/// Opens `url` as a chromeless "app" window with a Chromium-based browser
/// (Chrome, Edge, Chromium, Brave) if one is found, otherwise falls back to the
/// default browser.
pub(super) fn ui_app(url: &str) -> Result<(), String> {
    let arg = format!("--app={}", url);
    if cfg!(target_os = "macos") {
        for app in ["Google Chrome", "Microsoft Edge", "Chromium", "Brave Browser"] {
            if Command::new("open").args(["-na", app, "--args", &arg]).status().is_ok_and(|s| s.success()) {
                return Ok(());
            }
        }
    } else {
        const CANDIDATES: &[&str] = &[
            "google-chrome", "google-chrome-stable", "chromium", "chromium-browser", "chrome", "msedge",
            "microsoft-edge", "brave-browser", "brave",
        ];
        for exe in CANDIDATES {
            if Command::new(exe).arg(&arg).spawn().is_ok() {
                return Ok(());
            }
        }
    }
    ui_open(url)
}

/// Decodes `%XX` escapes in a URL piece (and `+` as a space, for query strings).
/// Invalid escapes are kept as written; invalid UTF-8 is replaced.
#[cfg_attr(not(feature = "native"), allow(dead_code))]
pub(super) fn url_unescape(s: &str, plus_is_space: bool) -> String {
    let b = s.as_bytes();
    let mut out = Vec::with_capacity(b.len());
    let mut i = 0;
    while i < b.len() {
        let c = b[i];
        if c == b'%' && i + 2 < b.len() {
            let hex = |x: u8| (x as char).to_digit(16);
            if let (Some(h), Some(l)) = (hex(b[i + 1]), hex(b[i + 2])) {
                out.push((h * 16 + l) as u8);
                i += 3;
                continue;
            }
        }
        out.push(if c == b'+' && plus_is_space { b' ' } else { c });
        i += 1;
    }
    String::from_utf8_lossy(&out).into_owned()
}

/// The content type to serve a file as, from its extension.
#[cfg(feature = "native")]
fn mime_of(path: &std::path::Path) -> &'static str {
    match path.extension().and_then(|e| e.to_str()).unwrap_or("").to_ascii_lowercase().as_str() {
        "html" | "htm" => "text/html; charset=utf-8",
        "css" => "text/css; charset=utf-8",
        "js" | "mjs" => "text/javascript; charset=utf-8",
        "json" => "application/json; charset=utf-8",
        "svg" => "image/svg+xml",
        "png" => "image/png",
        "jpg" | "jpeg" => "image/jpeg",
        "gif" => "image/gif",
        "webp" => "image/webp",
        "ico" => "image/x-icon",
        "wasm" => "application/wasm",
        "woff2" => "font/woff2",
        "woff" => "font/woff",
        "ttf" => "font/ttf",
        "mp3" => "audio/mpeg",
        "wav" => "audio/wav",
        "mp4" => "video/mp4",
        "txt" | "md" | "gw" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// The response for the static file a request `url` names inside `dir`:
/// `(status, content type, body)`. `/` and folders ending in `/` serve their
/// `index.html`; `..`, empty segments and drive letters are refused (403); a
/// missing file is a 404.
#[cfg(feature = "native")]
pub(super) fn static_file(dir: &str, url: &str) -> (u16, &'static str, Vec<u8>) {
    const TEXT: &str = "text/plain; charset=utf-8";
    let path = url.split(['?', '#']).next().unwrap_or("/");
    let mut rel = url_unescape(path.trim_start_matches('/'), false);
    if rel.is_empty() || rel.ends_with('/') {
        rel.push_str("index.html");
    }
    if rel.split(['/', '\\']).any(|seg| seg == ".." || seg.is_empty()) || rel.contains(':') {
        return (403, TEXT, b"403 Forbidden".to_vec());
    }
    let full = std::path::Path::new(dir).join(&rel);
    match std::fs::read(&full) {
        Ok(bytes) => (200, mime_of(&full), bytes),
        Err(_) => (404, TEXT, b"404 Not Found".to_vec()),
    }
}

/// Serves the files in `dir` on `127.0.0.1:port` with the right content types,
/// opens the browser on it, and keeps serving (this call does not return).
#[cfg(feature = "native")]
pub(super) fn ui_serve(dir: &str, port: i64) -> Result<(), String> {
    if !(1..=65535).contains(&port) {
        return Err("ui.serve: the port must be between 1 and 65535".to_string());
    }
    if !std::path::Path::new(dir).is_dir() {
        return Err(format!("ui.serve: \"{}\" is not a folder", dir));
    }
    let server = tiny_http::Server::http(("127.0.0.1", port as u16))
        .map_err(|e| format!("ui.serve: cannot listen on port {} ({})", port, e))?;
    let _ = ui_open(&format!("http://localhost:{}/", port)); // the port is bound; show it
    for req in server.incoming_requests() {
        let (code, ct, body) = static_file(dir, req.url());
        let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).expect("valid header");
        let _ = req.respond(tiny_http::Response::from_data(body).with_status_code(code).with_header(header));
    }
    Ok(())
}

#[cfg(not(feature = "native"))]
pub(super) fn ui_serve(_dir: &str, _port: i64) -> Result<(), String> {
    Err("ui.serve is not available in the web playground".to_string())
}
