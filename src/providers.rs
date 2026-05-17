use bytes::Bytes;
use serde_json::Value;

use crate::models::ChatRequest;

// ── Provider enum ─────────────────────────────────────────────────────────────

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Provider {
    OpenAI,
    Anthropic,
    Gemini,
}

impl Provider {
    pub fn from_str(s: &str) -> Self {
        match s.trim().to_ascii_lowercase().as_str() {
            "anthropic" | "claude" => Self::Anthropic,
            "gemini" | "google"   => Self::Gemini,
            _                     => Self::OpenAI,
        }
    }

    /// Full POST URL for chat completions on this provider.
    pub fn completions_url(&self, base_url: &str) -> String {
        match self {
            Self::OpenAI   => format!("{}/v1/chat/completions", base_url),
            // Gemini exposes an OpenAI-compatible surface under /v1beta/openai/
            Self::Gemini   => format!("{}/v1beta/openai/chat/completions", base_url),
            Self::Anthropic => format!("{}/v1/messages", base_url),
        }
    }

    /// Extra headers required by the provider beyond Authorization + Content-Type.
    pub fn extra_headers(&self) -> &'static [(&'static str, &'static str)] {
        match self {
            Self::Anthropic => &[("anthropic-version", "2023-06-01")],
            _ => &[],
        }
    }

    /// Translate an OpenAI-format ChatRequest into the provider's wire body.
    pub fn build_request(&self, req: &ChatRequest) -> Value {
        match self {
            Self::OpenAI | Self::Gemini => serde_json::to_value(req).unwrap_or_default(),
            Self::Anthropic => build_anthropic_body(req),
        }
    }

    /// Extract assistant text from a buffered (non-streaming) upstream response.
    pub fn parse_buffered(&self, data: &[u8]) -> String {
        match self {
            Self::OpenAI | Self::Gemini => parse_openai_json(data),
            Self::Anthropic => parse_anthropic_json(data),
        }
    }

    /// Extract assistant text from a complete, buffered SSE response body.
    pub fn parse_streaming(&self, data: &[u8]) -> String {
        match self {
            Self::OpenAI | Self::Gemini => parse_openai_sse(data),
            Self::Anthropic => parse_anthropic_sse(data),
        }
    }

    /// Synthesize an OpenAI-format non-streaming JSON response wrapping `content`.
    pub fn synthesize_json(&self, content: &str, model: &str) -> Value {
        serde_json::json!({
            "id":      format!("chatcmpl-{}", uuid::Uuid::new_v4()),
            "object":  "chat.completion",
            "created": chrono::Utc::now().timestamp(),
            "model":   model,
            "choices": [{
                "index":         0,
                "message":       {"role": "assistant", "content": content},
                "finish_reason": "stop"
            }],
            "usage": {"prompt_tokens": 0, "completion_tokens": 0, "total_tokens": 0}
        })
    }

    /// Synthesize an OpenAI-format SSE stream (role → content → stop → [DONE]).
    ///
    /// For Anthropic upstream, the actual streaming is buffered server-side
    /// then re-emitted as a single burst in OpenAI SSE format so the caller's
    /// OpenAI SDK keeps working without any client-side changes.
    pub fn synthesize_sse(&self, content: &str, model: &str) -> Bytes {
        let id = format!("chatcmpl-{}", uuid::Uuid::new_v4());
        let ts = chrono::Utc::now().timestamp();

        let role_chunk = serde_json::json!({
            "id": id, "object": "chat.completion.chunk", "created": ts, "model": model,
            "choices": [{"index": 0, "delta": {"role": "assistant"}, "finish_reason": null}]
        });
        let content_chunk = serde_json::json!({
            "id": id, "object": "chat.completion.chunk", "created": ts, "model": model,
            "choices": [{"index": 0, "delta": {"content": content}, "finish_reason": null}]
        });
        let stop_chunk = serde_json::json!({
            "id": id, "object": "chat.completion.chunk", "created": ts, "model": model,
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}]
        });

        Bytes::from(format!(
            "data: {}\n\ndata: {}\n\ndata: {}\n\ndata: [DONE]\n\n",
            serde_json::to_string(&role_chunk).unwrap(),
            serde_json::to_string(&content_chunk).unwrap(),
            serde_json::to_string(&stop_chunk).unwrap(),
        ))
    }
}

// ── Anthropic request builder ─────────────────────────────────────────────────

fn build_anthropic_body(req: &ChatRequest) -> Value {
    // System messages become the top-level `system` field.
    let system: String = req.messages.iter()
        .filter(|m| m.role == "system")
        .map(|m| m.content_text())
        .collect::<Vec<_>>()
        .join("\n\n");

    // Non-system messages must alternate user/assistant.
    // Merge consecutive same-role messages to satisfy Anthropic's constraint.
    let mut messages: Vec<Value> = Vec::new();
    for msg in req.messages.iter().filter(|m| m.role != "system") {
        let role = if msg.role == "assistant" { "assistant" } else { "user" };
        let text = msg.content_text();

        if let Some(last) = messages.last_mut() {
            if last["role"].as_str() == Some(role) {
                let prev = last["content"].as_str().unwrap_or("").to_string();
                last["content"] = Value::String(format!("{}\n\n{}", prev, text));
                continue;
            }
        }
        messages.push(serde_json::json!({"role": role, "content": text}));
    }

    let mut body = serde_json::json!({
        "model":      req.model,
        "messages":   messages,
        "max_tokens": req.max_tokens.unwrap_or(4096),
    });

    if !system.is_empty() {
        body["system"] = Value::String(system);
    }
    if let Some(temp) = req.temperature {
        if let Some(n) = serde_json::Number::from_f64(temp as f64) {
            body["temperature"] = Value::Number(n);
        }
    }
    if req.stream.unwrap_or(false) {
        body["stream"] = Value::Bool(true);
    }

    body
}

// ── OpenAI / Gemini response parsers ─────────────────────────────────────────

pub fn parse_openai_sse(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut content = String::new();
    for line in text.lines() {
        if let Some(payload) = line.strip_prefix("data: ") {
            if payload == "[DONE]" {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<Value>(payload) {
                if let Some(c) = json["choices"][0]["delta"]["content"].as_str() {
                    content.push_str(c);
                }
            }
        }
    }
    content
}

pub fn parse_openai_json(data: &[u8]) -> String {
    serde_json::from_slice::<Value>(data)
        .ok()
        .and_then(|v| {
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

// ── Anthropic response parsers ────────────────────────────────────────────────

fn parse_anthropic_sse(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut content = String::new();
    for line in text.lines() {
        if let Some(payload) = line.strip_prefix("data: ") {
            if let Ok(json) = serde_json::from_str::<Value>(payload) {
                if json["type"].as_str() == Some("content_block_delta")
                    && json["delta"]["type"].as_str() == Some("text_delta")
                {
                    if let Some(t) = json["delta"]["text"].as_str() {
                        content.push_str(t);
                    }
                }
            }
        }
    }
    content
}

fn parse_anthropic_json(data: &[u8]) -> String {
    serde_json::from_slice::<Value>(data)
        .ok()
        .and_then(|v| {
            v["content"].as_array()?
                .iter()
                .find(|b| b["type"].as_str() == Some("text"))?
                ["text"]
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}

#[cfg(test)]
mod tests {
    use super::*;

    // ── OpenAI / Gemini ───────────────────────────────────────────────────────

    #[test]
    fn openai_sse_concatenates_delta_content() {
        let sse = b"data: {\"choices\":[{\"delta\":{\"content\":\"Hello\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\", world\"}}]}\n\n\
                    data: [DONE]\n\n";
        assert_eq!(parse_openai_sse(sse), "Hello, world");
    }

    #[test]
    fn openai_sse_empty_on_done_only() {
        assert_eq!(parse_openai_sse(b"data: [DONE]\n\n"), "");
    }

    #[test]
    fn openai_sse_skips_chunks_without_content() {
        // role-only chunk has no content key
        let sse = b"data: {\"choices\":[{\"delta\":{\"role\":\"assistant\"}}]}\n\n\
                    data: {\"choices\":[{\"delta\":{\"content\":\"Hi\"}}]}\n\n\
                    data: [DONE]\n\n";
        assert_eq!(parse_openai_sse(sse), "Hi");
    }

    #[test]
    fn openai_json_extracts_message_content() {
        let json = br#"{"choices":[{"message":{"role":"assistant","content":"Test response"}}]}"#;
        assert_eq!(parse_openai_json(json), "Test response");
    }

    #[test]
    fn openai_json_returns_empty_on_malformed() {
        assert_eq!(parse_openai_json(b"not json"), "");
        assert_eq!(parse_openai_json(b"{}"), "");
    }

    // ── Anthropic ─────────────────────────────────────────────────────────────

    #[test]
    fn anthropic_sse_concatenates_text_deltas() {
        let sse = b"data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\"Hello\"}}\n\
                    data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"text_delta\",\"text\":\" Claude\"}}\n";
        assert_eq!(parse_anthropic_sse(sse), "Hello Claude");
    }

    #[test]
    fn anthropic_sse_ignores_non_text_event_types() {
        let sse = b"data: {\"type\":\"message_start\"}\n\
                    data: {\"type\":\"content_block_delta\",\"delta\":{\"type\":\"input_json_delta\",\"partial_json\":\"{}\"}}\n\
                    data: {\"type\":\"message_delta\"}\n";
        assert_eq!(parse_anthropic_sse(sse), "");
    }

    #[test]
    fn anthropic_json_extracts_first_text_block() {
        let json = br#"{"content":[{"type":"text","text":"Anthropic response"}]}"#;
        assert_eq!(parse_anthropic_json(json), "Anthropic response");
    }

    #[test]
    fn anthropic_json_skips_non_text_blocks() {
        let json = br#"{"content":[{"type":"tool_use","id":"x"},{"type":"text","text":"After tool"}]}"#;
        assert_eq!(parse_anthropic_json(json), "After tool");
    }

    // ── Anthropic request builder ─────────────────────────────────────────────

    #[test]
    fn build_anthropic_body_lifts_system_messages() {
        use crate::models::{ChatRequest, Message};
        let req = ChatRequest {
            model: "claude-3".into(),
            messages: vec![
                Message { role: "system".into(), content: "You are helpful.".into(), name: None },
                Message { role: "user".into(),   content: "Hello".into(),            name: None },
            ],
            stream: None,
            temperature: None,
            max_tokens: None,
            extra: Default::default(),
        };
        let body = build_anthropic_body(&req);
        assert_eq!(body["system"].as_str(), Some("You are helpful."));
        let msgs = body["messages"].as_array().unwrap();
        assert_eq!(msgs.len(), 1);
        assert_eq!(msgs[0]["role"].as_str(), Some("user"));
    }

    #[test]
    fn build_anthropic_body_merges_consecutive_same_role() {
        use crate::models::{ChatRequest, Message};
        let req = ChatRequest {
            model: "claude-3".into(),
            messages: vec![
                Message { role: "user".into(), content: "First".into(), name: None },
                Message { role: "user".into(), content: "Second".into(), name: None },
                Message { role: "assistant".into(), content: "Reply".into(), name: None },
            ],
            stream: None,
            temperature: None,
            max_tokens: None,
            extra: Default::default(),
        };
        let body = build_anthropic_body(&req);
        let msgs = body["messages"].as_array().unwrap();
        // Two consecutive user messages must be merged into one
        assert_eq!(msgs.len(), 2);
        assert!(msgs[0]["content"].as_str().unwrap().contains("First"));
        assert!(msgs[0]["content"].as_str().unwrap().contains("Second"));
    }
}
