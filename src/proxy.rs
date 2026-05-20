use axum::{
    body::Body,
    extract::State,
    http::{HeaderMap, StatusCode},
    response::Response,
};
use bytes::Bytes;
use futures::StreamExt;
use tokio::sync::mpsc;
use tokio_stream::wrappers::ReceiverStream;
use tracing::{error, info, warn};

use crate::{
    memory::{
        extraction::extract_and_store,
        retrieval::{build_injection, retrieve_relevant},
        store,
    },
    models::{ChatRequest, Message},
    providers::Provider,
    AppState,
};

// ── Public handlers ───────────────────────────────────────────────────────────

pub async fn handle_chat_completions(
    State(state): State<AppState>,
    headers: HeaderMap,
    body: Bytes,
) -> Result<Response<Body>, (StatusCode, String)> {
    // ── 1. Tenant context ─────────────────────────────────────────────────────
    let agent_id = headers
        .get("x-agent-id")
        .and_then(|v| v.to_str().ok())
        .ok_or((
            StatusCode::BAD_REQUEST,
            "Missing required header: x-agent-id".to_string(),
        ))?
        .to_string();

    let session_id = headers
        .get("x-session-id")
        .and_then(|v| v.to_str().ok())
        .map(|s| s.to_string())
        .unwrap_or_else(|| format!("sess-{}", uuid::Uuid::new_v4()));

    let importance_override: Option<f32> = headers
        .get("x-memory-importance")
        .and_then(|v| v.to_str().ok())
        .and_then(|s| s.parse::<f32>().ok())
        .map(|v| v.clamp(0.0, 1.0));

    // ── 2. Rate limiting ──────────────────────────────────────────────────────
    if !state.rate_limiter.check_and_consume(&agent_id) {
        state.metrics.rate_limited_total.inc();
        return Err((
            StatusCode::TOO_MANY_REQUESTS,
            format!(
                "{{\"error\":{{\"message\":\"Rate limit exceeded for agent {}\",\"type\":\"rate_limit_error\"}}}}",
                agent_id
            ),
        ));
    }

    // ── 3. Parse request ──────────────────────────────────────────────────────
    let mut chat_req: ChatRequest =
        serde_json::from_slice(&body).map_err(|e| (StatusCode::BAD_REQUEST, e.to_string()))?;

    if let Err(e) = store::upsert_agent(&state, &agent_id).await {
        warn!(agent_id = %agent_id, "upsert_agent: {}", e);
    }

    // ── 4. Memory retrieval + injection ───────────────────────────────────────
    let user_message = chat_req
        .messages
        .iter()
        .rev()
        .find(|m| m.role == "user")
        .map(|m| m.content_text())
        .unwrap_or_default();

    let original_messages = chat_req.messages.clone();
    let turn_number = original_messages.len() as i32;

    if !user_message.is_empty() {
        match retrieve_relevant(
            &state,
            &agent_id,
            &user_message,
            5,
            state.config.retrieval_threshold,
        )
        .await
        {
            Ok(memories) => {
                let mem_count = memories.len();
                if mem_count > 0 {
                    let wm = store::get_working_memory(&state, &agent_id, &session_id)
                        .await
                        .ok()
                        .flatten();
                    let injection = build_injection(&memories, wm.as_ref());

                    if !injection.is_empty() {
                        info!(agent_id = %agent_id, count = mem_count, "Injecting memories");
                        chat_req.messages.insert(
                            0,
                            Message {
                                role: "system".to_string(),
                                content: injection.into(),
                                name: None,
                            },
                        );
                        state.metrics.injection_total.with_label_values(&["hit"]).inc();
                        state.metrics.injected_per_req.observe(mem_count as f64);
                    } else {
                        state.metrics.injection_total.with_label_values(&["miss"]).inc();
                    }
                } else {
                    state.metrics.injection_total.with_label_values(&["miss"]).inc();
                }
            }
            Err(e) => {
                warn!(agent_id = %agent_id, "Memory retrieval skipped: {}", e);
                state.metrics.injection_total.with_label_values(&["miss"]).inc();
            }
        }
    }

    // ── 5. Build + send upstream request ─────────────────────────────────────
    let upstream_url = state.provider.completions_url(&state.config.upstream_base_url);
    let request_body = state.provider.build_request(&chat_req);

    let auth_header = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let mut req_builder = state
        .http_client
        .post(&upstream_url)
        .header("Authorization", &auth_header)
        .header("Content-Type", "application/json");

    for (name, value) in state.provider.extra_headers() {
        req_builder = req_builder.header(*name, *value);
    }

    let upstream_resp = req_builder
        .json(&request_body)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Upstream unreachable: {}", e)))?;

    let up_status  = upstream_resp.status();
    let up_headers = upstream_resp.headers().clone();
    let is_streaming = chat_req.stream.unwrap_or(false);
    let model = chat_req.model.clone();

    // ── 6. Dispatch by provider ───────────────────────────────────────────────
    match state.provider {
        Provider::Anthropic => {
            proxy_anthropic(
                state, agent_id, session_id, original_messages,
                turn_number, upstream_resp, up_status, is_streaming, model,
                importance_override,
            )
            .await
        }
        _ => {
            if is_streaming {
                proxy_streaming(
                    state, agent_id, session_id, original_messages,
                    turn_number, upstream_resp, up_status, up_headers,
                    importance_override,
                )
                .await
            } else {
                proxy_buffered(
                    state, agent_id, session_id, original_messages,
                    turn_number, upstream_resp, up_status, up_headers,
                    importance_override,
                )
                .await
            }
        }
    }
}

/// Transparent pass-through for GET /v1/models.
pub async fn handle_models(
    State(state): State<AppState>,
    headers: HeaderMap,
) -> Result<Response<Body>, (StatusCode, String)> {
    let auth = headers
        .get("authorization")
        .and_then(|v| v.to_str().ok())
        .unwrap_or("")
        .to_string();

    let models_url = format!("{}/v1/models", state.config.upstream_base_url);
    let resp = state
        .http_client
        .get(&models_url)
        .header("Authorization", &auth)
        .send()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    let status = resp.status();
    let bytes = resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, e.to_string()))?;

    Ok(Response::builder()
        .status(status)
        .header("content-type", "application/json")
        .body(Body::from(bytes))
        .expect("static headers; infallible"))
}

// ── OpenAI / Gemini streaming path ───────────────────────────────────────────

async fn proxy_streaming(
    state: AppState,
    agent_id: String,
    session_id: String,
    original_messages: Vec<Message>,
    turn_number: i32,
    upstream_resp: reqwest::Response,
    up_status: reqwest::StatusCode,
    up_headers: reqwest::header::HeaderMap,
    importance_override: Option<f32>,
) -> Result<Response<Body>, (StatusCode, String)> {
    let mut byte_stream = upstream_resp.bytes_stream();
    let (forward_tx, forward_rx) = mpsc::channel::<Bytes>(128);
    let (capture_tx, capture_rx) = tokio::sync::oneshot::channel::<Vec<u8>>();

    tokio::spawn(async move {
        let mut captured: Vec<u8> = Vec::new();
        while let Some(chunk) = byte_stream.next().await {
            match chunk {
                Ok(bytes) => {
                    captured.extend_from_slice(&bytes);
                    if forward_tx.send(bytes).await.is_err() {
                        break;
                    }
                }
                Err(e) => {
                    error!("Upstream stream error: {}", e);
                    break;
                }
            }
        }
        let _ = capture_tx.send(captured);
    });

    tokio::spawn(async move {
        if let Ok(captured) = capture_rx.await {
            let content = state.provider.parse_streaming(&captured);
            if !content.is_empty() {
                extract_and_store(
                    state,
                    agent_id,
                    session_id,
                    original_messages,
                    content,
                    turn_number,
                    importance_override,
                )
                .await;
            }
        }
    });

    let client_stream = ReceiverStream::new(forward_rx).map(Ok::<_, std::io::Error>);
    let mut builder = Response::builder().status(up_status.as_u16());
    for (k, v) in &up_headers {
        if k.as_str().eq_ignore_ascii_case("content-length") {
            continue;
        }
        builder = builder.header(k.as_str(), v);
    }
    Ok(builder.body(Body::from_stream(client_stream)).expect("response builder; infallible"))
}

// ── OpenAI / Gemini buffered path ─────────────────────────────────────────────

async fn proxy_buffered(
    state: AppState,
    agent_id: String,
    session_id: String,
    original_messages: Vec<Message>,
    turn_number: i32,
    upstream_resp: reqwest::Response,
    up_status: reqwest::StatusCode,
    up_headers: reqwest::header::HeaderMap,
    importance_override: Option<f32>,
) -> Result<Response<Body>, (StatusCode, String)> {
    let bytes = upstream_resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to read upstream: {}", e)))?;

    let bytes_clone = bytes.clone();
    tokio::spawn(async move {
        let content = state.provider.parse_buffered(&bytes_clone);
        if !content.is_empty() {
            extract_and_store(
                state,
                agent_id,
                session_id,
                original_messages,
                content,
                turn_number,
                importance_override,
            )
            .await;
        }
    });

    let mut builder = Response::builder().status(up_status.as_u16());
    for (k, v) in &up_headers {
        if k.as_str().eq_ignore_ascii_case("transfer-encoding") {
            continue;
        }
        builder = builder.header(k.as_str(), v);
    }
    Ok(builder.body(Body::from(bytes)).expect("response builder; infallible"))
}

// ── Anthropic path (buffer + synthesize OpenAI response) ──────────────────────
//
// Anthropic's wire format (request AND response) differs from OpenAI's.
// To keep callers using the OpenAI SDK with zero changes, we:
//   1. Buffer the entire Anthropic response (streaming or not).
//   2. Extract the assistant text.
//   3. Spawn memory extraction as usual.
//   4. Re-emit a synthetic OpenAI-format response so the caller's SDK is happy.
//
// Trade-off: first-byte latency equals full upstream latency rather than true
// token-by-token streaming.  Acceptable for agent use cases (batch-style calls).

async fn proxy_anthropic(
    state: AppState,
    agent_id: String,
    session_id: String,
    original_messages: Vec<Message>,
    turn_number: i32,
    upstream_resp: reqwest::Response,
    up_status: reqwest::StatusCode,
    is_streaming: bool,
    model: String,
    importance_override: Option<f32>,
) -> Result<Response<Body>, (StatusCode, String)> {
    // Forward non-2xx errors as-is (Anthropic error JSON is informative).
    if !up_status.is_success() {
        let status = StatusCode::from_u16(up_status.as_u16())
            .unwrap_or(StatusCode::BAD_GATEWAY);
        let body = upstream_resp.bytes().await.unwrap_or_default();
        return Ok(Response::builder()
            .status(status)
            .header("content-type", "application/json")
            .body(Body::from(body))
            .expect("static headers; infallible"));
    }

    let bytes = upstream_resp
        .bytes()
        .await
        .map_err(|e| (StatusCode::BAD_GATEWAY, format!("Failed to read Anthropic response: {}", e)))?;

    let content = if is_streaming {
        state.provider.parse_streaming(&bytes)
    } else {
        state.provider.parse_buffered(&bytes)
    };

    if !content.is_empty() {
        let state_c = state.clone();
        let agent_id_c  = agent_id.clone();
        let session_id_c = session_id.clone();
        let messages_c  = original_messages.clone();
        let content_c   = content.clone();
        tokio::spawn(async move {
            extract_and_store(state_c, agent_id_c, session_id_c, messages_c, content_c, turn_number, importance_override).await;
        });
    }

    if is_streaming {
        let sse = state.provider.synthesize_sse(&content, &model);
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "text/event-stream")
            .header("cache-control", "no-cache")
            .header("x-accel-buffering", "no")
            .body(Body::from(sse))
            .expect("static headers; infallible"))
    } else {
        let json_body = state.provider.synthesize_json(&content, &model);
        let json_bytes = serde_json::to_vec(&json_body)
            .map_err(|e| (StatusCode::INTERNAL_SERVER_ERROR, e.to_string()))?;
        Ok(Response::builder()
            .status(StatusCode::OK)
            .header("content-type", "application/json")
            .body(Body::from(json_bytes))
            .expect("static headers; infallible"))
    }
}
