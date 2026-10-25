//! The external-library package manager behind the `gwget` command.
//!
//! The catalogue is a single JSON index (Homebrew-style: a git repo holding one
//! index file). Each entry names a library, its version, a one-line description
//! and the raw URLs of its `.gw` files. `gwget install` downloads those files
//! into `~/.gw/libs/`, where `import` finds them, and records what it put there
//! in `~/.gw/installed.json`, so it can list and remove them later.

use std::collections::BTreeMap;
use std::path::{Path, PathBuf};

use serde_json::Value;

/// One library in the catalogue index.
#[derive(Debug, Clone)]
pub struct Entry {
    pub version: String,
    pub desc: String,
    pub files: Vec<String>,
}

/// The parsed catalogue: library name -> entry.
#[derive(Debug, Default)]
pub struct Index(pub BTreeMap<String, Entry>);

impl Index {
    pub fn get(&self, name: &str) -> Option<&Entry> {
        self.0.get(name)
    }
}

/// Parses the catalogue index: JSON of `name -> {version, desc, files:[url,...]}`.
pub fn parse_index(text: &str) -> Result<Index, String> {
    let v: Value = serde_json::from_str(text).map_err(|e| format!("the catalogue is not valid JSON: {}", e))?;
    let obj = v.as_object().ok_or("the catalogue must be a JSON object of libraries")?;
    let mut out = BTreeMap::new();
    for (name, e) in obj {
        let version = e.get("version").and_then(Value::as_str).unwrap_or("").to_string();
        let desc = e.get("desc").and_then(Value::as_str).unwrap_or("").to_string();
        let files: Vec<String> = e
            .get("files")
            .and_then(Value::as_array)
            .map(|a| a.iter().filter_map(|x| x.as_str().map(str::to_string)).collect())
            .unwrap_or_default();
        if files.is_empty() {
            return Err(format!("library \"{}\" in the catalogue lists no files", name));
        }
        out.insert(name.clone(), Entry { version, desc, files });
    }
    Ok(Index(out))
}

/// A library already installed, as recorded in installed.json.
#[derive(Debug, Clone)]
pub struct InstalledLib {
    pub version: String,
    pub files: Vec<String>,
}

/// The record of installed libraries.
#[derive(Debug, Default)]
pub struct Installed(pub BTreeMap<String, InstalledLib>);

/// Where `gwget` puts the `.gw` files it installs.
pub fn libs_dir(home: &Path) -> PathBuf {
    home.join("libs")
}

fn installed_path(home: &Path) -> PathBuf {
    home.join("installed.json")
}

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

/// The basename a URL's file is saved under; it must be a `something.gw`.
fn file_name_of(url: &str) -> Option<String> {
    let base = url.rsplit('/').next()?.split(['?', '#']).next()?;
    if base.ends_with(".gw") && base.len() > 3 {
        Some(base.to_string())
    } else {
        None
    }
}

/// What an install did, for the message `gwget` prints afterwards.
pub struct Report {
    pub name: String,
    pub version: String,
    pub files: Vec<String>,
    pub replaced: bool,
}

/// Installs a library from the index, downloading each of its files with `fetch`
/// (kept as an argument so the logic can be tested without the network).
pub fn install(
    home: &Path,
    name: &str,
    index: &Index,
    fetch: impl Fn(&str) -> Result<String, String>,
) -> Result<Report, String> {
    let entry = index.get(name).ok_or_else(|| format!("no library called \"{}\" in the catalogue", name))?;
    let dir = libs_dir(home);
    std::fs::create_dir_all(&dir).map_err(|e| format!("cannot create {}: {}", dir.display(), e))?;
    let mut saved: Vec<String> = Vec::new();
    for url in &entry.files {
        let fname = file_name_of(url).ok_or_else(|| format!("\"{}\" does not look like a .gw file URL", url))?;
        let body = fetch(url)?;
        std::fs::write(dir.join(&fname), body).map_err(|e| format!("cannot write {}: {}", fname, e))?;
        if !saved.contains(&fname) {
            saved.push(fname);
        }
    }
    let mut inst = read_installed(home);
    let replaced = inst.0.contains_key(name);
    // drop any file a previous version shipped but this one no longer does
    if let Some(old) = inst.0.get(name) {
        for f in &old.files {
            if !saved.contains(f) {
                let _ = std::fs::remove_file(dir.join(f));
            }
        }
    }
    inst.0.insert(name.to_string(), InstalledLib { version: entry.version.clone(), files: saved.clone() });
    write_installed(home, &inst)?;
    Ok(Report { name: name.to_string(), version: entry.version.clone(), files: saved, replaced })
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

/// Catalogue entries whose name or description contains `q` (case-insensitive).
pub fn search<'a>(index: &'a Index, q: &str) -> Vec<(&'a str, &'a Entry)> {
    let q = q.to_lowercase();
    index
        .0
        .iter()
        .filter(|(n, e)| n.to_lowercase().contains(&q) || e.desc.to_lowercase().contains(&q))
        .map(|(n, e)| (n.as_str(), e))
        .collect()
}

#[cfg(test)]
mod tests {
    use super::*;

    fn idx() -> Index {
        parse_index(
            r#"{
              "vec2":  {"version":"1.0.0","desc":"2D vectors","files":["https://x/vec2.gw"]},
              "mathx": {"version":"0.3.1","desc":"extra math helpers","files":["https://x/mathx.gw"]}
            }"#,
        )
        .unwrap()
    }

    #[test]
    fn parses_index() {
        let i = idx();
        assert_eq!(i.0.len(), 2);
        assert_eq!(i.get("vec2").unwrap().version, "1.0.0");
        assert_eq!(i.get("vec2").unwrap().files, vec!["https://x/vec2.gw"]);
    }

    #[test]
    fn rejects_bad_json() {
        assert!(parse_index("not json").is_err());
    }

    #[test]
    fn rejects_entry_without_files() {
        assert!(parse_index(r#"{"x":{"version":"1"}}"#).is_err());
    }

    #[test]
    fn searches_name_and_desc() {
        let i = idx();
        assert_eq!(search(&i, "math").len(), 1);
        assert_eq!(search(&i, "math")[0].0, "mathx");
        assert_eq!(search(&i, "vectors")[0].0, "vec2");
        assert_eq!(search(&i, "nothing").len(), 0);
    }

    #[test]
    fn file_name_rules() {
        assert_eq!(file_name_of("https://x/y/vec2.gw").as_deref(), Some("vec2.gw"));
        assert_eq!(file_name_of("https://x/vec2.gw?v=1").as_deref(), Some("vec2.gw"));
        assert_eq!(file_name_of("https://x/vec2.txt"), None);
        assert_eq!(file_name_of("https://x/.gw"), None);
    }

    #[test]
    fn install_list_remove_roundtrip() {
        let tmp = std::env::temp_dir().join(format!("gwget_test_{}", std::process::id()));
        let _ = std::fs::remove_dir_all(&tmp);
        let i = idx();

        let r = install(&tmp, "vec2", &i, |url| {
            assert!(url.ends_with("vec2.gw"));
            Ok("' vec2 library\n".to_string())
        })
        .unwrap();
        assert_eq!(r.version, "1.0.0");
        assert!(!r.replaced);
        assert!(libs_dir(&tmp).join("vec2.gw").is_file());
        assert_eq!(list(&tmp), vec![("vec2".to_string(), "1.0.0".to_string())]);

        // reinstall marks it as replaced
        let r2 = install(&tmp, "vec2", &i, |_| Ok("' v2\n".to_string())).unwrap();
        assert!(r2.replaced);

        remove(&tmp, "vec2").unwrap();
        assert!(!libs_dir(&tmp).join("vec2.gw").exists());
        assert!(list(&tmp).is_empty());
        assert!(remove(&tmp, "vec2").is_err());

        let _ = std::fs::remove_dir_all(&tmp);
    }

    #[test]
    fn install_unknown_fails() {
        let tmp = std::env::temp_dir().join(format!("gwget_unknown_{}", std::process::id()));
        let i = idx();
        assert!(install(&tmp, "nope", &i, |_| Ok(String::new())).is_err());
    }
}
