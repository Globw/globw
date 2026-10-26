//! The external-library package manager behind the `gwget` command.
//!
//! The catalogue is a single online table (Supabase): `gwget publish` adds or
//! updates a library instantly and anonymously; `gwget install` / `search` read
//! it; the maintainer moderates (block / remove) from the Supabase dashboard.
//! A library's files live in the row itself. `gwget install` writes them into
//! `~/.gw/libs/`, where `import` finds them.
//!
//! This module holds the logic that can be tested without the network (reading
//! a manifest, shaping the publish request, parsing rows, the local library
//! folder and the edit-token store). The HTTP calls live in `src/bin/gwget.rs`.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

// ---------------------------------------------------------------------------
// the local library folder (~/.gw/libs) and what is installed there
// ---------------------------------------------------------------------------

/// Where `gwget` writes the `.gw` files it installs.
pub fn libs_dir(home: &Path) -> PathBuf {
    home.join("libs")
}

fn installed_path(home: &Path) -> PathBuf {
    home.join("installed.json")
}

/// A library installed on this machine, as recorded in installed.json.
#[derive(Debug, Clone)]
pub struct InstalledLib {
    pub version: String,
    pub files: Vec<String>,
}

/// The record of installed libraries.
#[derive(Debug, Default)]
pub struct Installed(pub BTreeMap<String, InstalledLib>);

/// Reads installed.json, or an empty record if it is missing or unreadable.
pub fn read_installed(home: &Path) -> Installed {
    let text = match std::fs::read_to_string(installed_path(home)) {
        Ok(t) => t,
        Err(_) => return Installed::default(),
    };
    let v: Value = match serde_json::from_str(&text) {
        Ok(v) => v,
        Err(_) => return Installed::default(),
    };
    let mut out = BTreeMap::new();
    if let Some(obj) = v.as_object() {
        for (name, e) in obj {
            let version = e.get("version").and_then(Value::as_str).unwrap_or("").to_string();
            let files = e
                .get("files")
                .and_then(Value::as_array)
                .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
                .unwrap_or_default();
            out.insert(name.clone(), InstalledLib { version, files });
        }
    }
    Installed(out)
}

fn write_installed(home: &Path, inst: &Installed) -> Result<(), String> {
    let obj: serde_json::Map<String, Value> = inst
        .0
        .iter()
        .map(|(name, l)| (name.clone(), serde_json::json!({ "version": l.version, "files": l.files })))
        .collect();
    let text = serde_json::to_string_pretty(&Value::Object(obj)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(home).map_err(|e| format!("cannot create {}: {}", home.display(), e))?;
    std::fs::write(installed_path(home), text).map_err(|e| format!("cannot write installed.json: {}", e))
}

/// What an install did, for the message `gwget` prints afterwards.
pub struct Report {
    pub name: String,
    pub version: String,
    pub files: Vec<String>,
    pub replaced: bool,
}

/// A `.gw` file name is one path segment ending in `.gw`, no separators.
pub fn valid_file_name(name: &str) -> bool {
    name.len() > 3
        && name.ends_with(".gw")
        && !name.contains('/')
        && !name.contains('\\')
        && !name.contains("..")
        && name.chars().all(|c| c.is_ascii_alphanumeric() || matches!(c, '_' | '-' | '.'))
}

/// A library name: lower-case word, so it maps straight to `import <name>`.
pub fn valid_lib_name(name: &str) -> bool {
    (1..=40).contains(&name.len())
        && name.chars().all(|c| c.is_ascii_lowercase() || c.is_ascii_digit() || c == '_')
        && name.starts_with(|c: char| c.is_ascii_lowercase())
}

/// Writes a downloaded library's files into `libs/` and records it, dropping any
/// file a previous version left behind.
pub fn install_files(home: &Path, name: &str, version: &str, files: &[(String, String)]) -> Result<Report, String> {
    if files.is_empty() {
        return Err(format!("library \"{}\" has no files", name));
    }
    for (fname, _) in files {
        if !valid_file_name(fname) {
            return Err(format!("library \"{}\" has an unsafe file name: \"{}\"", name, fname));
        }
    }
    let dir = libs_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    let mut saved: Vec<String> = Vec::new();
    for (fname, content) in files {
        std::fs::write(dir.join(fname), content).map_err(|e| format!("cannot write {}: {}", fname, e))?;
        if !saved.contains(fname) {
            saved.push(fname.clone());
        }
    }
    let mut inst = read_installed(home);
    let replaced = inst.0.contains_key(name);
    if let Some(old) = inst.0.get(name) {
        for f in &old.files {
            if !saved.contains(f) {
                let _ = std::fs::remove_file(dir.join(f));
            }
        }
    }
    inst.0.insert(name.to_string(), InstalledLib { version: version.to_string(), files: saved.clone() });
    write_installed(home, &inst)?;
    Ok(Report { name: name.to_string(), version: version.to_string(), files: saved, replaced })
}

/// Removes an installed library and the files it put in `libs/`.
pub fn remove(home: &Path, name: &str) -> Result<(), String> {
    let mut inst = read_installed(home);
    let lib = inst.0.remove(name).ok_or_else(|| format!("\"{}\" is not installed", name))?;
    let dir = libs_dir(home);
    for f in &lib.files {
        let _ = std::fs::remove_file(dir.join(f));
    }
    write_installed(home, &inst)
}

/// The installed libraries, as (name, version) pairs, sorted by name.
pub fn list(home: &Path) -> Vec<(String, String)> {
    read_installed(home).0.into_iter().map(|(n, l)| (n, l.version)).collect()
}

// ---------------------------------------------------------------------------
// publishing: the manifest, the files of a library folder, the request body
// ---------------------------------------------------------------------------

/// A library's details, read from its `gw.json` manifest.
#[derive(Debug, Clone)]
pub struct Manifest {
    pub name: String,
    pub version: String,
    pub description: String,
    pub author: String,
}

/// Parses a `gw.json` manifest: `{name, version, description?, author?}`.
pub fn parse_manifest(text: &str) -> Result<Manifest, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("gw.json is not valid JSON: {}", e))?;
    let name = v.get("name").and_then(Value::as_str).unwrap_or("").to_string();
    if !valid_lib_name(&name) {
        return Err("the manifest needs a \"name\" (lower-case letters, digits and _, starting with a letter)".to_string());
    }
    let version = v.get("version").and_then(Value::as_str).unwrap_or("").to_string();
    if version.is_empty() {
        return Err("the manifest needs a \"version\", e.g. \"1.0.0\"".to_string());
    }
    let description = v.get("description").and_then(Value::as_str).unwrap_or("").to_string();
    let author = v.get("author").and_then(Value::as_str).unwrap_or("anonymous").to_string();
    Ok(Manifest { name, version, description, author })
}

/// Reads the `.gw` files directly inside `dir` into (file name, content) pairs,
/// sorted by name. The library must ship `<name>.gw` as its entry module.
pub fn collect_gw_files(dir: &Path, name: &str) -> Result<Vec<(String, String)>, String> {
    let entries = std::fs::read_dir(dir).map_err(|e| format!("cannot read {}: {}", dir.display(), e))?;
    let mut files: Vec<(String, String)> = Vec::new();
    for entry in entries.flatten() {
        let path = entry.path();
        if path.extension().and_then(|e| e.to_str()) != Some("gw") {
            continue;
        }
        let fname = path.file_name().and_then(|n| n.to_str()).unwrap_or("").to_string();
        if !valid_file_name(&fname) {
            continue;
        }
        let content = std::fs::read_to_string(&path).map_err(|e| format!("cannot read {}: {}", fname, e))?;
        files.push((fname, content));
    }
    files.sort_by(|a, b| a.0.cmp(&b.0));
    if !files.iter().any(|(f, _)| f == &format!("{}.gw", name)) {
        return Err(format!("the folder must contain {}.gw (the library's entry module)", name));
    }
    Ok(files)
}

/// The parameters for the `publish_library` RPC (files as a `{name: content}`
/// object, matching the server function's `jsonb` argument).
pub fn publish_params(m: &Manifest, files: &[(String, String)], token: &str) -> Value {
    let files_obj: serde_json::Map<String, Value> =
        files.iter().map(|(n, c)| (n.clone(), Value::String(c.clone()))).collect();
    serde_json::json!({
        "p_name": m.name,
        "p_version": m.version,
        "p_description": m.description,
        "p_author": m.author,
        "p_files": Value::Object(files_obj),
        "p_token": token,
    })
}

// ---------------------------------------------------------------------------
// reading catalogue rows (from the libraries_public view)
// ---------------------------------------------------------------------------

/// One library as returned by the catalogue.
#[derive(Debug, Clone)]
pub struct Lib {
    pub name: String,
    pub version: String,
    pub description: String,
    pub files: Vec<(String, String)>,
}

/// Parses the JSON array the catalogue returns into libraries.
pub fn parse_rows(json: &str) -> Result<Vec<Lib>, String> {
    let v: Value = serde_json::from_str(json).map_err(|e| format!("the catalogue sent invalid JSON: {}", e))?;
    let arr = v.as_array().ok_or("the catalogue did not send a list of libraries")?;
    let mut out = Vec::new();
    for row in arr {
        let name = row.get("name").and_then(Value::as_str).unwrap_or("").to_string();
        let version = row.get("version").and_then(Value::as_str).unwrap_or("").to_string();
        let description = row.get("description").and_then(Value::as_str).unwrap_or("").to_string();
        let files = row
            .get("files")
            .and_then(Value::as_object)
            .map(|o| o.iter().filter_map(|(n, c)| c.as_str().map(|s| (n.clone(), s.to_string()))).collect())
            .unwrap_or_default();
        out.push(Lib { name, version, description, files });
    }
    Ok(out)
}

// ---------------------------------------------------------------------------
// Supabase connection details and the per-library edit tokens
// ---------------------------------------------------------------------------

/// The catalogue's Supabase project (the anon key is public, safe to store).
#[derive(Debug, Clone)]
pub struct Config {
    pub url: String,
    pub key: String,
}

/// Loads the Supabase settings from the environment or `~/.gw/config.json`.
pub fn load_config(home: &Path) -> Option<Config> {
    let env = (std::env::var("GWGET_SUPABASE_URL").ok(), std::env::var("GWGET_SUPABASE_KEY").ok());
    if let (Some(url), Some(key)) = env {
        if !url.is_empty() && !key.is_empty() {
            return Some(Config { url: url.trim_end_matches('/').to_string(), key });
        }
    }
    let text = std::fs::read_to_string(home.join("config.json")).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    let url = v.get("supabase_url").and_then(Value::as_str)?.trim_end_matches('/').to_string();
    let key = v.get("supabase_anon_key").and_then(Value::as_str)?.to_string();
    if url.is_empty() || key.is_empty() {
        return None;
    }
    Some(Config { url, key })
}

fn tokens_path(home: &Path) -> PathBuf {
    home.join("tokens.json")
}

/// The saved edit token for a library published from this machine, if any.
pub fn read_token(home: &Path, name: &str) -> Option<String> {
    let text = std::fs::read_to_string(tokens_path(home)).ok()?;
    let v: Value = serde_json::from_str(&text).ok()?;
    v.get(name).and_then(Value::as_str).map(str::to_string)
}

/// Remembers a library's edit token so later `gwget publish` can update it.
pub fn save_token(home: &Path, name: &str, token: &str) -> Result<(), String> {
    let mut map = std::fs::read_to_string(tokens_path(home))
        .ok()
        .and_then(|t| serde_json::from_str::<Value>(&t).ok())
        .and_then(|v| v.as_object().cloned())
        .unwrap_or_default();
    map.insert(name.to_string(), Value::String(token.to_string()));
    let text = serde_json::to_string_pretty(&Value::Object(map)).map_err(|e| e.to_string())?;
    std::fs::create_dir_all(home).map_err(|e| format!("cannot create {}: {}", home.display(), e))?;
    std::fs::write(tokens_path(home), text).map_err(|e| format!("cannot write tokens.json: {}", e))
}

/// A fresh random edit token for a first-time publish.
pub fn new_token() -> String {
    uuid::Uuid::new_v4().simple().to_string()
}

/// Extracts a readable message from a PostgREST/Supabase error body, so the
/// catalogue's own rejection text (spam, copy, wrong token) is shown plainly
/// instead of raw JSON.
pub fn supabase_error(body: &str) -> Option<String> {
    let v: Value = serde_json::from_str(body).ok()?;
    let msg = v.get("message").and_then(Value::as_str)?.trim();
    if msg.is_empty() {
        return None;
    }
    match v.get("hint").and_then(Value::as_str) {
        Some(h) if !h.trim().is_empty() => Some(format!("{} ({})", msg, h.trim())),
        _ => Some(msg.to_string()),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn names() {
        assert!(valid_lib_name("vec2"));
        assert!(valid_lib_name("math_x"));
        assert!(!valid_lib_name("Vec2")); // upper-case
        assert!(!valid_lib_name("2cool")); // starts with a digit
        assert!(!valid_lib_name("a b")); // space
        assert!(valid_file_name("vec2.gw"));
        assert!(!valid_file_name("vec2.txt"));
        assert!(!valid_file_name("../evil.gw"));
        assert!(!valid_file_name("a/b.gw"));
    }

    #[test]
    fn parses_manifest() {
        let m = parse_manifest(r#"{"name":"vec2","version":"1.0.0","description":"2D vectors"}"#).unwrap();
        assert_eq!(m.name, "vec2");
        assert_eq!(m.version, "1.0.0");
        assert_eq!(m.author, "anonymous");
        assert!(parse_manifest(r#"{"name":"vec2"}"#).is_err()); // no version
        assert!(parse_manifest(r#"{"version":"1"}"#).is_err()); // no name
        assert!(parse_manifest(r#"{"name":"Bad","version":"1"}"#).is_err()); // bad name
    }

    #[test]
    fn publish_body_shape() {
        let m = Manifest {
            name: "vec2".into(),
            version: "1.0.0".into(),
            description: "d".into(),
            author: "me".into(),
        };
        let files = vec![("vec2.gw".to_string(), "' code".to_string())];
        let p = publish_params(&m, &files, "tok123");
        assert_eq!(p["p_name"], "vec2");
        assert_eq!(p["p_token"], "tok123");
        assert_eq!(p["p_files"]["vec2.gw"], "' code");
    }

    #[test]
    fn parses_rows() {
        let json = r#"[{"name":"vec2","version":"1.0.0","description":"2D","files":{"vec2.gw":"' code"}}]"#;
        let rows = parse_rows(json).unwrap();
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].name, "vec2");
        assert_eq!(rows[0].files, vec![("vec2.gw".to_string(), "' code".to_string())]);
        assert!(parse_rows("{}").is_err()); // not an array
    }

    #[test]
    fn install_list_remove_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("gwget_pkg_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let files = vec![("greet.gw".to_string(), "' greet\n".to_string())];
        let r = install_files(&tmp, "greet", "1.0.0", &files).unwrap();
        assert!(!r.replaced);
        assert!(libs_dir(&tmp).join("greet.gw").is_file());
        assert_eq!(list(&tmp), vec![("greet".to_string(), "1.0.0".to_string())]);
        let r2 = install_files(&tmp, "greet", "1.1.0", &files).unwrap();
        assert!(r2.replaced);
        remove(&tmp, "greet").unwrap();
        assert!(!libs_dir(&tmp).join("greet.gw").exists());
        assert!(list(&tmp).is_empty());
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn token_store_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("gwget_tok_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        assert!(read_token(&tmp, "vec2").is_none());
        save_token(&tmp, "vec2", "secret").unwrap();
        save_token(&tmp, "mathx", "other").unwrap();
        assert_eq!(read_token(&tmp, "vec2").as_deref(), Some("secret"));
        assert_eq!(read_token(&tmp, "mathx").as_deref(), Some("other"));
        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn new_token_is_long_and_hex() {
        let t = new_token();
        assert_eq!(t.len(), 32);
        assert!(t.chars().all(|c| c.is_ascii_hexdigit()));
    }

    #[test]
    fn reads_supabase_error() {
        assert_eq!(
            supabase_error(r#"{"message":"rejected: spam","hint":null,"details":null}"#).as_deref(),
            Some("rejected: spam")
        );
        assert_eq!(
            supabase_error(r#"{"message":"bad name","hint":"use lower-case"}"#).as_deref(),
            Some("bad name (use lower-case)")
        );
        assert_eq!(supabase_error("not json"), None);
        assert_eq!(supabase_error(r#"{"code":"x"}"#), None);
    }
}
