use anyhow::{anyhow, Result};
use crate::AppState;

/// Generates a vector embedding for `text` using the configured OpenAI-compatible
/// embeddings endpoint.  Returns an error instead of panicking so callers can
/// gracefully fall back (e.g. skip memory injection on a transient API hiccup).
pub async fn embed_text(state: &AppState, text: &str) -> Result<Vec<f32>> {
    let api_key = state
        .config
        .openai_api_key
        .as_deref()
        .unwrap_or("no-key");

    let payload = serde_json::json!({
        "input": text,
        "model": state.config.embedding_model,
    });

    let response = state
        .http_client
        .post(format!("{}/v1/embeddings", state.config.upstream_base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let status = response.status();
        let body = response.text().await.unwrap_or_default();
        return Err(anyhow!("Embedding API returned {}: {}", status, body));
    }

    let body: serde_json::Value = response.json().await?;

    let embedding = body["data"][0]["embedding"]
        .as_array()
        .ok_or_else(|| anyhow!("Unexpected embedding response shape: {:?}", body))?
        .iter()
        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
        .collect();

    Ok(embedding)
}
