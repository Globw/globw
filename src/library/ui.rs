//! The "ui" module's helpers: open a URL in the browser, so a Globw program can
//! serve a web interface (HTML/CSS/JS) with the `serve` module and show it. The
//! page is the UI, Globw is the logic.

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
        "txt" | "md" => "text/plain; charset=utf-8",
        _ => "application/octet-stream",
    }
}

/// Serves the files in `dir` on `127.0.0.1:port` with the right content types,
/// opens the browser on it, and keeps serving (this call does not return). `/`
/// serves `index.html`. Path traversal (`..`) is refused.
#[cfg(feature = "native")]
pub(super) fn ui_serve(dir: &str, port: i64) -> Result<(), String> {
    let server = tiny_http::Server::http(("127.0.0.1", port as u16))
        .map_err(|e| format!("ui.serve: cannot listen on port {} ({})", port, e))?;
    let root = std::path::Path::new(dir);
    let _ = ui_open(&format!("http://localhost:{}/", port)); // the port is bound; show it
    for req in server.incoming_requests() {
        let url = req.url().split('?').next().unwrap_or("/").to_string();
        let rel = url.trim_start_matches('/');
        let rel = if rel.is_empty() { "index.html" } else { rel };
        let safe = !rel.split('/').any(|seg| seg == ".." || seg.is_empty());
        let resp = if safe {
            match std::fs::read(root.join(rel)) {
                Ok(bytes) => {
                    let ct = mime_of(&root.join(rel));
                    let header = tiny_http::Header::from_bytes(&b"Content-Type"[..], ct.as_bytes()).unwrap();
                    tiny_http::Response::from_data(bytes).with_header(header)
                }
                Err(_) => tiny_http::Response::from_data(b"404 Not Found".to_vec()).with_status_code(404),
            }
        } else {
            tiny_http::Response::from_data(b"403 Forbidden".to_vec()).with_status_code(403)
        };
        let _ = req.respond(resp);
    }
    Ok(())
}

#[cfg(not(feature = "native"))]
pub(super) fn ui_serve(_dir: &str, _port: i64) -> Result<(), String> {
    Err("ui.serve is not available in the web playground".to_string())
}
