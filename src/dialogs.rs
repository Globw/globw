//! Cross-platform dialog boxes (Windows, Linux, macOS) via `tinyfiledialogs`:
//! a message, a Yes/No question and a text input box. On Linux the native box
//! comes from zenity / kdialog / matedialog when one is installed, otherwise the
//! terminal; on macOS from osascript; on Windows from the system dialog API.

use tinyfiledialogs::{input_box, message_box_ok, message_box_yes_no, MessageBoxIcon, YesNo};

/// Shows an information box with an OK button.
pub fn message(text: &str) -> Result<(), String> {
    message_box_ok("Globw", &sanitize(text), MessageBoxIcon::Info);
    Ok(())
}

/// Shows a Yes/No question; `true` means Yes. Defaults to No.
pub fn confirm(question: &str) -> Result<bool, String> {
    Ok(message_box_yes_no("Globw", &sanitize(question), MessageBoxIcon::Question, YesNo::No) == YesNo::Yes)
}

/// Shows an input box; returns the text typed, or "" if cancelled.
pub fn ask(question: &str) -> Result<String, String> {
    Ok(input_box("Globw", &sanitize(question), "").unwrap_or_default())
}

/// `tinyfiledialogs` builds a shell command on Linux/macOS, so a quote or a
/// newline in the text could break the box; swap them for safe equivalents.
fn sanitize(text: &str) -> String {
    text.replace(['"', '\''], "`").replace(['\n', '\r'], " ")
}
