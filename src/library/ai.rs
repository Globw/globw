//! The "ai" module's helpers: chat and embeddings against an OpenAI-compatible endpoint.
use std::cell::RefCell;
use serde_json::Value as Json;
#[cfg(not(feature = "native"))]
use super::NET_OFF;

// Groq, OpenRouter, OpenAI and a local Ollama all speak the same
// POST {endpoint}/chat/completions API, so one module covers paid keys and
// local models (Ollama needs no key).
pub(super) struct AiConfig {
    pub endpoint: String,
    pub key: String,
    pub model: String,
    pub system: String,
    pub temperature: f64,
    pub history: Vec<(String, String)>,
    pub max_tokens: Option<i64>,
    pub top_p: Option<f64>,
    pub seed: Option<i64>,
    pub stop: Option<String>,
    pub timeout: u64,
    pub last: String,
    pub last_tokens: i64,
}

impl Default for AiConfig {
    fn default() -> Self {
        AiConfig {
            endpoint: "http://localhost:11434/v1".to_string(), // Ollama by default
            key: String::new(),
            model: "llama3".to_string(),
            system: String::new(),
            temperature: 0.7,
            history: Vec::new(),
            max_tokens: None,
            top_p: None,
            seed: None,
            stop: None,
            timeout: 180,
            last: String::new(),
            last_tokens: 0,
        }
    }
}

thread_local! {
    pub(super) static AI: RefCell<AiConfig> = RefCell::new(AiConfig::default());
}

#[cfg(not(feature = "native"))]
pub(super) fn ai_send(_cfg: &AiConfig, _messages: Vec<Json>, _json_mode: bool) -> Result<(String, i64), String> {
    Err(NET_OFF.to_string())
}
#[cfg(not(feature = "native"))]
pub(super) fn ai_embed(_cfg: &AiConfig, _input: &str) -> Result<Vec<f64>, String> {
    Err(NET_OFF.to_string())
}
#[cfg(not(feature = "native"))]
pub(super) fn ai_models(_cfg: &AiConfig) -> Result<Vec<String>, String> {
    Err(NET_OFF.to_string())
}

// One POST to an OpenAI-compatible endpoint; returns the parsed JSON reply.
#[cfg(feature = "native")]
pub(super) fn ai_post(cfg: &AiConfig, path: &str, body: &Json) -> Result<Json, String> {
    let url = format!("{}/{}", cfg.endpoint.trim_end_matches('/'), path);
    let body = serde_json::to_string(body).map_err(|e| format!("ai: cannot build the request: {}", e))?;
    let mut req = ureq::post(&url)
        .timeout(std::time::Duration::from_secs(cfg.timeout))
        .set("Content-Type", "application/json");
    if !cfg.key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", cfg.key));
    }
    let resp = req.send_string(&body).map_err(|e| match e {
        ureq::Error::Status(code, r) => {
            format!("ai: the server answered with status {} - {}", code, r.into_string().unwrap_or_default())
        }
        ureq::Error::Transport(t) => format!("ai: cannot reach {}: {}", url, t),
    })?;
    let text = resp.into_string().map_err(|e| format!("ai: cannot read the reply: {}", e))?;
    serde_json::from_str(&text).map_err(|e| format!("ai: the reply is not JSON: {}", e))
}

// Chat completion. Returns (answer, total_tokens). json_mode asks for a JSON object.
#[cfg(feature = "native")]
pub(super) fn ai_send(cfg: &AiConfig, messages: Vec<Json>, json_mode: bool) -> Result<(String, i64), String> {
    let mut body = serde_json::json!({
        "model": cfg.model,
        "messages": messages,
        "temperature": cfg.temperature,
    });
    let m = body.as_object_mut().unwrap();
    if let Some(n) = cfg.max_tokens {
        m.insert("max_tokens".into(), serde_json::json!(n));
    }
    if let Some(p) = cfg.top_p {
        m.insert("top_p".into(), serde_json::json!(p));
    }
    if let Some(s) = cfg.seed {
        m.insert("seed".into(), serde_json::json!(s));
    }
    if let Some(stop) = &cfg.stop {
        m.insert("stop".into(), serde_json::json!(stop));
    }
    if json_mode {
        m.insert("response_format".into(), serde_json::json!({ "type": "json_object" }));
    }
    let json = ai_post(cfg, "chat/completions", &body)?;
    let content = json["choices"][0]["message"]["content"]
        .as_str()
        .map(str::to_string)
        .ok_or_else(|| format!("ai: no answer in the reply: {}", json))?;
    let tokens = json["usage"]["total_tokens"].as_i64().unwrap_or(0);
    Ok((content, tokens))
}

// Embed one text into a vector (the /embeddings endpoint).
#[cfg(feature = "native")]
pub(super) fn ai_embed(cfg: &AiConfig, input: &str) -> Result<Vec<f64>, String> {
    let body = serde_json::json!({ "model": cfg.model, "input": input });
    let json = ai_post(cfg, "embeddings", &body)?;
    let arr = json["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| format!("ai: no embedding in the reply: {}", json))?;
    Ok(arr.iter().map(|v| v.as_f64().unwrap_or(0.0)).collect())
}

// List the models the endpoint offers (the /models endpoint).
#[cfg(feature = "native")]
pub(super) fn ai_models(cfg: &AiConfig) -> Result<Vec<String>, String> {
    let url = format!("{}/models", cfg.endpoint.trim_end_matches('/'));
    let mut req = ureq::get(&url).timeout(std::time::Duration::from_secs(cfg.timeout));
    if !cfg.key.is_empty() {
        req = req.set("Authorization", &format!("Bearer {}", cfg.key));
    }
    let resp = req.call().map_err(|e| match e {
        ureq::Error::Status(code, r) => {
            format!("ai: the server answered with status {} - {}", code, r.into_string().unwrap_or_default())
        }
        ureq::Error::Transport(t) => format!("ai: cannot reach {}: {}", url, t),
    })?;
    let text = resp.into_string().map_err(|e| format!("ai: cannot read the reply: {}", e))?;
    let json: Json = serde_json::from_str(&text).map_err(|e| format!("ai: the reply is not JSON: {}", e))?;
    let arr = json["data"].as_array().ok_or_else(|| format!("ai: no models in the reply: {}", json))?;
    Ok(arr.iter().filter_map(|m| m["id"].as_str().map(str::to_string)).collect())
}

pub(super) fn ai_messages(cfg: &AiConfig, extra: Option<&str>) -> Vec<Json> {
    let mut messages = Vec::new();
    if !cfg.system.is_empty() {
        messages.push(serde_json::json!({ "role": "system", "content": cfg.system }));
    }
    for (role, content) in &cfg.history {
        messages.push(serde_json::json!({ "role": role, "content": content }));
    }
    if let Some(prompt) = extra {
        messages.push(serde_json::json!({ "role": "user", "content": prompt }));
    }
    messages
}
