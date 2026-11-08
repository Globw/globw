//! Helpers for the "net"/"web" HTTP modules: build a request (native) and parse
//! a URL into host / path / query. The builtins live in `call()`.

/// Sends an HTTP request of any method, with an optional single request header and
/// body, for the richer `net` module.
#[cfg(feature = "native")]
pub(super) fn net_build(method: &str, url: &str, body: &str, header: Option<(&str, &str)>) -> Result<ureq::Response, ureq::Error> {
    let mut req = ureq::request(&method.to_uppercase(), url).timeout(std::time::Duration::from_secs(30));
    if let Some((name, value)) = header {
        req = req.set(name, value);
    }
    if body.is_empty() {
        req.call()
    } else {
        req.send_string(body)
    }
}

pub(super) fn url_host(url: &str) -> String {
    let s = url.split("://").nth(1).unwrap_or(url);
    let authority = s.split(['/', '?', '#']).next().unwrap_or("");
    let authority = authority.rsplit('@').next().unwrap_or(authority);
    authority.split(':').next().unwrap_or(authority).to_string()
}

pub(super) fn url_path(url: &str) -> String {
    let s = url.split("://").nth(1).unwrap_or(url);
    match s.find('/') {
        Some(i) => s[i..].split(['?', '#']).next().unwrap_or("/").to_string(),
        None => "/".to_string(),
    }
}

pub(super) fn url_query(url: &str, key: &str) -> String {
    let Some((_, q)) = url.split_once('?') else {
        return String::new();
    };
    let q = q.split('#').next().unwrap_or(q);
    for pair in q.split('&') {
        let (k, v) = pair.split_once('=').unwrap_or((pair, ""));
        if k == key {
            return v.to_string();
        }
    }
    String::new()
}
