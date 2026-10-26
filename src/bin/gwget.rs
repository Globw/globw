//! `gwget`: the package manager for external Globw libraries.
//!
//! The catalogue is an online Supabase table. `gwget publish` adds or updates a
//! library instantly and anonymously (a secret edit token, printed on the first
//! publish, protects its name afterwards). `gwget install` / `search` read the
//! catalogue; the maintainer moderates (block / remove) from the dashboard.
//!
//! Connection settings come from the environment (GWGET_SUPABASE_URL,
//! GWGET_SUPABASE_KEY) or `~/.gw/config.json`; the anon key is public. GW_HOME
//! overrides the `~/.gw` folder.

use std::path::{Path, PathBuf};
use std::process::ExitCode;

use globw::pkg;

/// The Globw home folder (default `~/.gw`), overridable with GW_HOME.
fn gw_home() -> Option<PathBuf> {
    if let Some(h) = std::env::var_os("GW_HOME").filter(|h| !h.is_empty()) {
        return Some(PathBuf::from(h));
    }
    let home = std::env::var_os("USERPROFILE").or_else(|| std::env::var_os("HOME")).filter(|h| !h.is_empty())?;
    Some(PathBuf::from(home).join(".gw"))
}

/// Percent-encodes a value placed inside a PostgREST query.
fn enc(s: &str) -> String {
    let mut out = String::new();
    for b in s.bytes() {
        if b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.' | b'~') {
            out.push(b as char);
        } else {
            out.push_str(&format!("%{:02X}", b));
        }
    }
    out
}

fn read_resp(resp: Result<ureq::Response, ureq::Error>) -> Result<String, String> {
    match resp {
        Ok(r) => r.into_string().map_err(|e| format!("cannot read the reply: {}", e)),
        Err(ureq::Error::Status(code, r)) => {
            let body = r.into_string().unwrap_or_default();
            let msg = pkg::supabase_error(&body).unwrap_or_else(|| body.trim().to_string());
            Err(format!("the catalogue refused the request ({}): {}", code, msg))
        }
        Err(e) => Err(format!("cannot reach the catalogue: {}", e)),
    }
}

fn sb_get(cfg: &pkg::Config, path_query: &str) -> Result<String, String> {
    let url = format!("{}/rest/v1/{}", cfg.url, path_query);
    read_resp(
        ureq::get(&url)
            .set("apikey", &cfg.key)
            .set("Authorization", &format!("Bearer {}", cfg.key))
            .timeout(std::time::Duration::from_secs(60))
            .call(),
    )
}

fn sb_rpc(cfg: &pkg::Config, func: &str, body: serde_json::Value) -> Result<String, String> {
    let url = format!("{}/rest/v1/rpc/{}", cfg.url, func);
    read_resp(
        ureq::post(&url)
            .set("apikey", &cfg.key)
            .set("Authorization", &format!("Bearer {}", cfg.key))
            .set("Content-Type", "application/json")
            .timeout(std::time::Duration::from_secs(60))
            .send_string(&body.to_string()),
    )
}

fn config(home: &Path) -> Result<pkg::Config, String> {
    pkg::load_config(home).ok_or_else(|| {
        format!(
            "the catalogue is not set up yet. Either set GWGET_SUPABASE_URL and GWGET_SUPABASE_KEY, \
             or create {} with:\n  {{\n    \"supabase_url\": \"https://<project>.supabase.co\",\n    \
             \"supabase_anon_key\": \"<the public anon key>\"\n  }}",
            home.join("config.json").display()
        )
    })
}

fn do_publish(home: &Path, dir: &str) -> Result<(), String> {
    let cfg = config(home)?;
    let dir = Path::new(dir);
    let manifest_text = std::fs::read_to_string(dir.join("gw.json"))
        .map_err(|_| format!("no gw.json in {} (the library manifest)", dir.display()))?;
    let manifest = pkg::parse_manifest(&manifest_text)?;
    let files = pkg::collect_gw_files(dir, &manifest.name)?;

    let existing = pkg::read_token(home, &manifest.name);
    let token = existing.clone().unwrap_or_else(pkg::new_token);
    let body = pkg::publish_params(&manifest, &files, &token);
    let reply = sb_rpc(&cfg, "publish_library", body)?;
    let what = reply.trim().trim_matches('"');

    pkg::save_token(home, &manifest.name, &token)?;
    println!("{} {} {} ({} file{})", what, manifest.name, manifest.version, files.len(), if files.len() == 1 { "" } else { "s" });
    println!("it is online now:  gwget install {}", manifest.name);
    if existing.is_none() {
        println!("\nyour edit token (keep it safe \u{2014} you need it to update this library):\n  {}", token);
    }
    Ok(())
}

fn do_install(home: &Path, name: &str) -> Result<(), String> {
    let cfg = config(home)?;
    let pq = format!("libraries_public?name=eq.{}&select=name,version,files", enc(name));
    let rows = pkg::parse_rows(&sb_get(&cfg, &pq)?)?;
    let lib = rows.into_iter().next().ok_or_else(|| format!("no library called \"{}\" in the catalogue", name))?;
    let r = pkg::install_files(home, &lib.name, &lib.version, &lib.files)?;
    let what = if r.replaced { "updated" } else { "installed" };
    println!("{} {} {} ({} file{})", what, r.name, r.version, r.files.len(), if r.files.len() == 1 { "" } else { "s" });
    println!("use it with:  import {}", r.name);
    Ok(())
}

fn do_search(home: &Path, q: &str) -> Result<(), String> {
    let cfg = config(home)?;
    let e = enc(q);
    let pq = format!(
        "libraries_public?select=name,version,description&or=(name.ilike.*{0}*,description.ilike.*{0}*)&limit=50",
        e
    );
    let rows = pkg::parse_rows(&sb_get(&cfg, &pq)?)?;
    if rows.is_empty() {
        println!("nothing in the catalogue matches \"{}\"", q);
    } else {
        for lib in rows {
            println!("{:<20} {:<10} {}", lib.name, lib.version, lib.description);
        }
    }
    Ok(())
}

fn run() -> Result<(), String> {
    let args: Vec<String> = std::env::args().skip(1).collect();
    let home = gw_home().ok_or("cannot find your home folder; set GW_HOME to choose where libraries go")?;
    let a: Vec<&str> = args.iter().map(String::as_str).collect();
    match a.as_slice() {
        [] | ["help"] | ["-h"] | ["--help"] => {
            println!("{}", help());
            Ok(())
        }
        ["publish"] => do_publish(&home, "."),
        ["publish", dir] => do_publish(&home, dir),
        ["install", name] | ["add", name] => do_install(&home, name),
        ["search", q] => do_search(&home, q),
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
        ["where"] => {
            println!("{}", pkg::libs_dir(&home).display());
            Ok(())
        }
        _ => Err(format!("unknown command \"{}\". Run \"gwget help\".", a.join(" "))),
    }
}

fn help() -> String {
    concat!(
        "Usage:\n",
        "  gwget <command> [arguments]\n",
        "\n",
        "Commands:\n",
        "  install <name>    Install a library from the catalogue into ~/.gw/libs.\n",
        "  search <text>     Search the catalogue by name or description.\n",
        "  publish [dir]     Publish the library in <dir> (default: the current folder).\n",
        "  remove <name>     Uninstall a library from this machine.\n",
        "  list              List the libraries installed here.\n",
        "  where             Print the libraries folder.\n",
        "  help              Show this help.\n",
        "\n",
        "Publishing:\n",
        "  Reads gw.json (name, version, description, author) and the .gw files in the\n",
        "  folder. A library goes online at once; the first publish prints a one-time\n",
        "  edit token, which you need to update it later.\n",
        "\n",
        "Configuration:\n",
        "  GWGET_SUPABASE_URL and GWGET_SUPABASE_KEY, or ~/.gw/config.json.\n",
        "  GW_HOME moves the ~/.gw folder."
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
