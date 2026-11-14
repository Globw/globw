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
