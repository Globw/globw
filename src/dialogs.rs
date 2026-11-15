//! Cross-platform dialog boxes (Windows, Linux, macOS) via `tinyfiledialogs`:
//! a message, a Yes/No question, a text input box, and choosing a file to open
//! or to save, a folder or a colour. Without the `native`
//! feature (the wasm playground) there is no desktop, so they report that.

#[cfg(feature = "native")]
mod desktop {
    use tinyfiledialogs::{
        color_chooser_dialog, input_box, message_box_ok, message_box_yes_no, open_file_dialog, save_file_dialog,
        save_file_dialog_with_filter, select_folder_dialog, DefaultColorValue, MessageBoxIcon, YesNo,
    };

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

    /// Asks for a file to open; "" if cancelled.
    pub fn open_file(title: &str, filter: &str) -> Result<String, String> {
        let (desc, patterns) = super::parse_filter(filter);
        let pats: Vec<String> = patterns.iter().map(|p| sanitize(p)).collect();
        let refs: Vec<&str> = pats.iter().map(String::as_str).collect();
        let desc = sanitize(&desc);
        let f = if refs.is_empty() { None } else { Some((&refs[..], desc.as_str())) };
        Ok(open_file_dialog(&sanitize(title), "", f).unwrap_or_default())
    }

    /// Asks where to save a file (suggesting `name`); "" if cancelled.
    pub fn save_file(title: &str, name: &str, filter: &str) -> Result<String, String> {
        let (desc, patterns) = super::parse_filter(filter);
        let pats: Vec<String> = patterns.iter().map(|p| sanitize(p)).collect();
        let refs: Vec<&str> = pats.iter().map(String::as_str).collect();
        let (title, name) = (sanitize(title), sanitize(name));
        Ok(if refs.is_empty() {
            save_file_dialog(&title, &name)
        } else {
            save_file_dialog_with_filter(&title, &name, &refs, &sanitize(&desc))
        }
        .unwrap_or_default())
    }

    /// Asks for a folder; "" if cancelled.
    pub fn choose_folder(title: &str) -> Result<String, String> {
        Ok(select_folder_dialog(&sanitize(title), "").unwrap_or_default())
    }

    /// Asks for a colour, starting from `current` ("#rrggbb"); "" if cancelled.
    pub fn choose_color(current: &str) -> Result<String, String> {
        let start = crate::canvas::parse_color(current).unwrap_or(0xFFFFFF);
        let hex = format!("#{:06x}", start & 0xFF_FFFF);
        Ok(color_chooser_dialog("Globw", DefaultColorValue::Hex(&hex))
            .map(|(_, [r, g, b])| format!("#{:02x}{:02x}{:02x}", r, g, b))
            .unwrap_or_default())
    }

    /// `tinyfiledialogs` builds a shell command on Linux/macOS, so a quote or a
    /// newline in the text could break the box; swap them for safe equivalents.
    /// A NUL byte would make it panic: it is dropped.
    fn sanitize(text: &str) -> String {
        text.replace(['"', '\''], "`").replace(['\n', '\r'], " ").replace('\0', "")
    }
}

#[cfg(feature = "native")]
pub use desktop::{ask, choose_color, choose_folder, confirm, message, open_file, save_file};

/// A file filter: "*.txt *.md" or "Images: *.png, *.jpg" -> (description, patterns).
pub fn parse_filter(filter: &str) -> (String, Vec<String>) {
    let (desc, pats) = match filter.split_once(':') {
        Some((d, p)) => (d.trim().to_string(), p),
        None => (String::new(), filter),
    };
    let patterns: Vec<String> = pats
        .split(|c: char| c == ',' || c == ';' || c.is_whitespace())
        .filter(|p| !p.is_empty())
        .map(str::to_string)
        .collect();
    let desc = if desc.is_empty() { patterns.join(" ") } else { desc };
    (desc, patterns)
}

#[cfg(not(feature = "native"))]
const NO_DIALOGS: &str = "dialogs only run on the desktop, not in the web playground";

#[cfg(not(feature = "native"))]
pub fn message(_text: &str) -> Result<(), String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn confirm(_question: &str) -> Result<bool, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn ask(_question: &str) -> Result<String, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn open_file(_title: &str, _filter: &str) -> Result<String, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn save_file(_title: &str, _name: &str, _filter: &str) -> Result<String, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn choose_folder(_title: &str) -> Result<String, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(not(feature = "native"))]
pub fn choose_color(_current: &str) -> Result<String, String> {
    Err(NO_DIALOGS.to_string())
}

#[cfg(test)]
mod tests {
    #[test]
    fn file_filters() {
        let v = |xs: &[&str]| xs.iter().map(|x| x.to_string()).collect::<Vec<_>>();
        assert_eq!(super::parse_filter(""), (String::new(), vec![]));
        assert_eq!(super::parse_filter("*.txt *.md"), ("*.txt *.md".to_string(), v(&["*.txt", "*.md"])));
        assert_eq!(super::parse_filter("Images: *.png, *.jpg"), ("Images".to_string(), v(&["*.png", "*.jpg"])));
    }
}
