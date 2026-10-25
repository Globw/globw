//! `gwget`: the package manager for external Globw libraries.
//!
//! The catalogue is a single JSON index in a git repo (Homebrew-style). To add
//! a library, its author opens a pull request to that repo adding one entry;
//! the maintainer reviews and merges it. `gwget install <name>` reads the index
//! and downloads the library's `.gw` files into `~/.gw/libs/`, where `import`
//! finds them. Override the catalogue URL (or a path) with GWGET_INDEX.

use std::path::PathBuf;
use std::process::ExitCode;

use globw::pkg;

/// The catalogue the project publishes. A placeholder until the repo exists;
/// set GWGET_INDEX to point somewhere else (a URL, or a local file for testing).
const DEFAULT_INDEX: &str = "https://raw.githubusercontent.com/globw-lang/registry/main/index.json";

/// The Globw home folder (default `~/.gw`), overridable with GW_HOME.
fn gw_home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("GW_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".gw"))
}

fn index_source() -> String {
    std::env::var("GWGET_INDEX").unwrap_or_else(|_| DEFAULT_INDEX.to_string())
}

/// Fetches a URL's text, or reads a local file when `src` is not http(s).
fn fetch(src: &str) -> Result<String, String> {
    if src.starts_with("http://") || src.starts_with("https://") {
        ureq::get(src)
            .timeout(std::time::Duration::from_secs(60))
            .call()
            .map_err(|e| format!("cannot download {}: {}", src, e))?
            .into_string()
            .map_err(|e| format!("cannot read the reply from {}: {}", src, e))
    } else {
        let p = src.strip_prefix("file://").unwrap_or(src);
        std::fs::read_to_string(p).map_err(|e| format!("cannot read {}: {}", p, e))
    }
}

fn load_index() -> Result<pkg::Index, String> {
    let src = index_source();
    let text = fetch(&src)?;
    pkg::parse_index(&text)
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = gw_home().ok_or("cannot find your home folder; set GW_HOME to choose where libraries go")?;
    match args.iter().map(String::as_str).collect::<Vec<_>>().as_slice() {
        ["install", name] | ["add", name] => {
            let index = load_index()?;
            let r = pkg::install(&home, name, &index, fetch)?;
            let what = if r.replaced { "updated" } else { "installed" };
            let plural = if r.files.len() == 1 { "" } else { "s" };
            println!("{} {} {} ({} file{})", what, r.name, r.version, r.files.len(), plural);
            println!("use it with:  import {}", r.name);
            Ok(())
        }
        ["remove", name] | ["uninstall", name] => {
            pkg::remove(&home, name)?;
            println!("removed {}", name);
            Ok(())
        }
        ["list"] => {
            let libs = pkg::list(&home);
            if libs.is_empty() {
                println!("no libraries installed");
            } else {
                for (n, v) in libs {
                    println!("{:<20} {}", n, v);
                }
            }
            Ok(())
        }
        ["search", q] => {
            let index = load_index()?;
            let hits = pkg::search(&index, q);
            if hits.is_empty() {
                println!("nothing in the catalogue matches \"{}\"", q);
            } else {
                for (n, e) in hits {
                    println!("{:<20} {:<10} {}", n, e.version, e.desc);
                }
            }
            Ok(())
        }
        ["where"] => {
            println!("{}", pkg::libs_dir(&home).display());
            Ok(())
        }
        _ => Err(usage()),
    }
}

fn usage() -> String {
    concat!(
        "Usage:\n",
        "  gwget install <name>   download a library into ~/.gw/libs\n",
        "  gwget remove  <name>   remove an installed library\n",
        "  gwget list             list installed libraries\n",
        "  gwget search <text>    search the catalogue\n",
        "  gwget where            print the libraries folder\n",
        "\n",
        "The catalogue URL (or a local path) can be set with GWGET_INDEX."
    )
    .to_string()
}

fn main() -> ExitCode {
    match run() {
        Ok(()) => ExitCode::SUCCESS,
        Err(e) => {
            eprintln!("gwget: {}", e);
            ExitCode::from(1)
        }
    }
}
