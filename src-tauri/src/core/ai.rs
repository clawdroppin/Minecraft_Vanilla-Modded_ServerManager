//! Optional AI assistant. Talks to a model on the user's own computer (or LAN) through the
//! OpenAI-compatible API that Ollama, LM Studio, llama.cpp's server, Jan and vLLM all offer.
//! Nothing is sent anywhere unless the user sets it up, and only to the URL they choose.
//!
//! This module only streams one completion at a time. The agent loop – the system prompt,
//! running tools, asking the user before changes – lives in the UI (`src/features/ai`), so the
//! tools reuse the exact commands the rest of the app uses.

use futures_util::StreamExt;
use serde::{Deserialize, Serialize};
use serde_json::{json, Value};
use std::collections::{BTreeMap, HashMap};
use std::sync::atomic::{AtomicBool, AtomicU64, Ordering};
use std::sync::{Arc, LazyLock, Mutex};
use std::time::Duration;
use tauri::ipc::Channel;

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq)]
#[serde(rename_all = "camelCase", default)]
pub struct AiSettings {
    /// OpenAI-compatible base URL, e.g. http://localhost:11434/v1 (Ollama).
    pub base_url: String,
    /// Empty = whatever the server lists first.
    pub model: String,
    /// Optional bearer token (LM Studio / vLLM with auth, or a LAN proxy).
    pub api_key: String,
    pub temperature: f32,
    /// ask | auto – whether changes need a click first. Reading is always allowed.
    pub approval: String,
    /// Offer the tools to the model. Off = plain chat (for models without tool support).
    pub tools: bool,
}

impl Default for AiSettings {
    fn default() -> Self {
        Self {
            base_url: "http://localhost:11434/v1".into(),
            model: String::new(),
            api_key: String::new(),
            temperature: 0.3,
            approval: "ask".into(),
            tools: true,
        }
    }
}

#[derive(Debug, Clone, Serialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct ToolCall {
    pub id: String,
    pub name: String,
    /// Raw JSON text, as the model wrote it.
    pub arguments: String,
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind", rename_all = "camelCase", rename_all_fields = "camelCase")]
pub enum Event {
    /// Answer text.
    Delta { text: String },
    /// Reasoning text from models that send it separately (`reasoning_content` / `reasoning`).
    Thinking { text: String },
    Done { tool_calls: Vec<ToolCall>, finish_reason: String, model: String },
    Error { text: String },
}

static NEXT: AtomicU64 = AtomicU64::new(1);
static STOPS: LazyLock<Mutex<HashMap<u64, Arc<AtomicBool>>>> = LazyLock::new(Default::default);

fn client() -> reqwest::Client {
    reqwest::Client::builder()
        .connect_timeout(Duration::from_secs(5))
        // Big prompts on a CPU can take minutes before the first token.
        .read_timeout(Duration::from_secs(600))
        .build()
        .unwrap_or_default()
}

pub fn endpoint(base_url: &str, path: &str) -> String {
    let base = base_url.trim().trim_end_matches('/');
    // People paste "http://localhost:11434" for Ollama; with no path at all the OpenAI API is /v1.
    let has_path = base.split_once("://").map(|(_, rest)| rest.contains('/')).unwrap_or(true);
    let base = if has_path { base.to_string() } else { format!("{base}/v1") };
    format!("{base}/{path}")
}

fn request(c: &reqwest::Client, method: reqwest::Method, url: String, key: &str) -> reqwest::RequestBuilder {
    let r = c.request(method, url);
    if key.trim().is_empty() { r } else { r.bearer_auth(key.trim()) }
}

fn connect_error(e: &reqwest::Error, base_url: &str) -> String {
    if e.is_connect() {
        format!("Couldn't reach an AI server at {base_url}. Is Ollama (or LM Studio's local server) running?")
    } else if e.is_timeout() {
        "The AI server took too long to answer.".into()
    } else {
        format!("Connection problem: {e}")
    }
}

/// The models the server offers, chat models first.
pub async fn list_models(s: &AiSettings) -> Result<Vec<String>, String> {
    let resp = request(&client(), reqwest::Method::GET, endpoint(&s.base_url, "models"), &s.api_key)
        .timeout(Duration::from_secs(10))
        .send()
        .await
        .map_err(|e| connect_error(&e, &s.base_url))?;
    let status = resp.status();
    let body = resp.text().await.map_err(|e| e.to_string())?;
    if !status.is_success() {
        return Err(format!("The AI server answered {status}: {}", shorten(&body, 200)));
    }
    let v: Value = serde_json::from_str(&body).map_err(|_| "The server's answer wasn't the expected JSON. Is this an OpenAI-compatible address (ending in /v1)?".to_string())?;
    let models: Vec<String> = v["data"].as_array().map(|l| l.iter().filter_map(|m| m["id"].as_str().map(String::from)).collect()).unwrap_or_default();
    let (mut chat, embed): (Vec<_>, Vec<_>) = models.into_iter().partition(|m| !m.to_lowercase().contains("embed"));
    chat.extend(embed);
    Ok(chat)
}

pub fn stop(id: u64) {
    if let Some(f) = STOPS.lock().unwrap().get(&id) {
        f.store(true, Ordering::Relaxed);
    }
}

/// Starts one streamed completion and returns its id (for `stop`). `messages` and `tools` are
/// passed through in OpenAI format, so the UI owns the conversation.
pub fn start_chat(s: AiSettings, messages: Vec<Value>, tools: Vec<Value>, channel: Channel<Event>) -> u64 {
    let id = NEXT.fetch_add(1, Ordering::Relaxed);
    let flag = Arc::new(AtomicBool::new(false));
    STOPS.lock().unwrap().insert(id, flag.clone());
    tauri::async_runtime::spawn(async move {
        let send = |e: Event| {
            let _ = channel.send(e);
        };
        if let Err(text) = run_chat(&s, messages, tools, &flag, &send).await {
            send(Event::Error { text });
        }
        STOPS.lock().unwrap().remove(&id);
    });
    id
}

async fn run_chat(s: &AiSettings, messages: Vec<Value>, tools: Vec<Value>, stop: &AtomicBool, send: &impl Fn(Event)) -> Result<(), String> {
    let mut model = s.model.trim().to_string();
    if model.is_empty() {
        model = list_models(s).await?.into_iter().next().ok_or("The AI server has no model loaded. Run `ollama pull qwen3:8b` (or load a model in LM Studio) first.")?;
    }
    let mut body = json!({ "model": model, "messages": messages, "stream": true, "temperature": s.temperature });
    if !tools.is_empty() {
        body["tools"] = Value::Array(tools);
    }
    let resp = request(&client(), reqwest::Method::POST, endpoint(&s.base_url, "chat/completions"), &s.api_key)
        .json(&body)
        .send()
        .await
        .map_err(|e| connect_error(&e, &s.base_url))?;
    let status = resp.status();
    if !status.is_success() {
        let text = resp.text().await.unwrap_or_default();
        let msg = serde_json::from_str::<Value>(&text).ok().and_then(|v| v["error"]["message"].as_str().or(v["error"].as_str()).map(String::from)).unwrap_or_else(|| shorten(&text, 300));
        return Err(format!("The AI server answered {status}: {msg}"));
    }

    let mut acc = Accumulator::default();
    let mut stream = resp.bytes_stream();
    let mut buf: Vec<u8> = vec![];
    'read: while let Some(chunk) = stream.next().await {
        if stop.load(Ordering::Relaxed) {
            acc.finish = "stopped".into();
            break;
        }
        buf.extend_from_slice(&chunk.map_err(|_| "The connection to the AI server was lost.".to_string())?);
        while let Some(pos) = buf.iter().position(|&b| b == b'\n') {
            let line: Vec<u8> = buf.drain(..=pos).collect();
            let line = String::from_utf8_lossy(&line);
            match acc.line(&line, send) {
                Ok(true) => break 'read,
                Ok(false) => {}
                Err(e) => return Err(e),
            }
        }
    }
    if !buf.is_empty() {
        let line = String::from_utf8_lossy(&buf).into_owned();
        acc.line(&line, send)?;
    }
    send(Event::Done { tool_calls: acc.calls(), finish_reason: acc.finish, model });
    Ok(())
}

/// Collects a streamed answer. Tool calls arrive in pieces keyed by `index`.
#[derive(Default)]
struct Accumulator {
    calls: BTreeMap<u64, ToolCall>,
    finish: String,
    /// Inside a `<think>` block a model wrote into the answer text.
    in_think: bool,
}

impl Accumulator {
    /// Handles one SSE line; `Ok(true)` = the stream is complete.
    fn line(&mut self, line: &str, send: &impl Fn(Event)) -> Result<bool, String> {
        let line = line.trim();
        let data = line.strip_prefix("data:").unwrap_or(line).trim();
        if data.is_empty() || data.starts_with(':') || data.starts_with("event:") {
            return Ok(false);
        }
        if data == "[DONE]" {
            return Ok(true);
        }
        let Ok(v) = serde_json::from_str::<Value>(data) else { return Ok(false) };
        if let Some(err) = v.get("error") {
            return Err(err["message"].as_str().map(String::from).unwrap_or_else(|| err.to_string()));
        }
        let choice = &v["choices"][0];
        // Servers that ignore `stream` send a whole `message` instead of `delta`s.
        let d = if choice["delta"].is_object() { &choice["delta"] } else { &choice["message"] };
        for key in ["reasoning_content", "reasoning"] {
            if let Some(t) = d[key].as_str().filter(|t| !t.is_empty()) {
                send(Event::Thinking { text: t.into() });
            }
        }
        if let Some(t) = d["content"].as_str().filter(|t| !t.is_empty()) {
            self.text(t, send);
        }
        if let Some(list) = d["tool_calls"].as_array() {
            for (n, tc) in list.iter().enumerate() {
                let idx = tc["index"].as_u64().unwrap_or(n as u64);
                let e = self.calls.entry(idx).or_default();
                if let Some(id) = tc["id"].as_str().filter(|s| !s.is_empty()) {
                    e.id = id.into();
                }
                if let Some(name) = tc["function"]["name"].as_str().filter(|s| !s.is_empty()) {
                    e.name = name.into();
                }
                match &tc["function"]["arguments"] {
                    Value::String(a) => e.arguments.push_str(a),
                    Value::Null => {}
                    // Some servers send the arguments as an object.
                    other => e.arguments = other.to_string(),
                }
            }
        }
        if let Some(f) = choice["finish_reason"].as_str() {
            self.finish = f.into();
        }
        Ok(false)
    }

    /// Splits `<think>…</think>` (Qwen, DeepSeek-R1 on some servers) from the answer.
    fn text(&mut self, mut t: &str, send: &impl Fn(Event)) {
        while !t.is_empty() {
            let tag = if self.in_think { "</think>" } else { "<think>" };
            let (before, after) = match t.find(tag) {
                Some(i) => (&t[..i], Some(&t[i + tag.len()..])),
                None => (t, None),
            };
            if !before.is_empty() {
                send(if self.in_think { Event::Thinking { text: before.into() } } else { Event::Delta { text: before.into() } });
            }
            match after {
                Some(rest) => {
                    self.in_think = !self.in_think;
                    t = rest;
                }
                None => break,
            }
        }
    }

    fn calls(&self) -> Vec<ToolCall> {
        self.calls
            .values()
            .filter(|c| !c.name.is_empty())
            .enumerate()
            .map(|(i, c)| ToolCall { id: if c.id.is_empty() { format!("call_{i}") } else { c.id.clone() }, ..c.clone() })
            .collect()
    }
}

pub fn shorten(text: &str, max: usize) -> String {
    let t = text.trim();
    if t.chars().count() <= max {
        return t.into();
    }
    format!("{}…", t.chars().take(max).collect::<String>())
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::cell::RefCell;

    #[test]
    fn events_use_camel_case_fields() {
        let e = Event::Done { tool_calls: vec![], finish_reason: "stop".into(), model: "m".into() };
        assert_eq!(serde_json::to_string(&e).unwrap(), r#"{"kind":"done","toolCalls":[],"finishReason":"stop","model":"m"}"#);
    }

    #[test]
    fn builds_endpoints() {
        assert_eq!(endpoint("http://localhost:1234/v1/", "models"), "http://localhost:1234/v1/models");
        assert_eq!(endpoint("http://localhost:11434", "models"), "http://localhost:11434/v1/models");
        assert_eq!(shorten("abcdef", 3), "abc…");
    }

    #[test]
    fn accumulates_streamed_tool_calls_and_thinking() {
        let out = RefCell::new(String::new());
        let think = RefCell::new(String::new());
        let send = |e: Event| match e {
            Event::Delta { text } => out.borrow_mut().push_str(&text),
            Event::Thinking { text } => think.borrow_mut().push_str(&text),
            _ => {}
        };
        let mut a = Accumulator::default();
        let lines = [
            r#"data: {"choices":[{"delta":{"content":"<think>hmm"}}]}"#,
            r#"data: {"choices":[{"delta":{"content":" ok</think>Hi"}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"id":"c1","function":{"name":"read_file","arguments":"{\"pa"}}]}}]}"#,
            r#"data: {"choices":[{"delta":{"tool_calls":[{"index":0,"function":{"arguments":"th\":\"a\"}"}}]},"finish_reason":"tool_calls"}]}"#,
        ];
        for l in lines {
            assert!(!a.line(l, &send).unwrap());
        }
        assert!(a.line("data: [DONE]", &send).unwrap());
        assert_eq!(out.borrow().as_str(), "Hi");
        assert_eq!(think.borrow().as_str(), "hmm ok");
        let calls = a.calls();
        assert_eq!(calls.len(), 1);
        assert_eq!(calls[0].name, "read_file");
        assert_eq!(calls[0].arguments, r#"{"path":"a"}"#);
        assert_eq!(a.finish, "tool_calls");
    }
}
