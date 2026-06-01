use crate::AppState;
use anyhow::{anyhow, Result};

fn embeddings_url(state: &AppState) -> String {
    format!("{}/v1/embeddings", state.config.embedding_base_url)
}

fn api_key(state: &AppState) -> &str {
    state.config.openai_api_key.as_deref().unwrap_or("no-key")
}

/// Embed multiple texts in a single API call.
///
/// The OpenAI embeddings endpoint accepts an array `input`; the response
/// includes an `index` field per object so results can be returned in the
/// same order as the input even if the server reorders them.
///
/// Returns an empty `Vec` when `texts` is empty.
pub async fn embed_texts(state: &AppState, texts: &[&str]) -> Result<Vec<Vec<f32>>> {
    if texts.is_empty() {
        return Ok(Vec::new());
    }

    let payload = serde_json::json!({
        "input": texts,
        "model": state.config.embedding_model,
    });

    let response = state
        .http_client
        .post(embeddings_url(state))
        .header("Authorization", format!("Bearer {}", api_key(state)))
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
    let data = body["data"]
        .as_array()
        .ok_or_else(|| anyhow!("Unexpected embedding response shape: {:?}", body))?;

    // Sort by index so the output order matches the input order.
    let mut indexed: Vec<(usize, Vec<f32>)> = data
        .iter()
        .map(|item| {
            let idx = item["index"].as_u64().unwrap_or(0) as usize;
            let vec = item["embedding"]
                .as_array()
                .map(|arr| {
                    arr.iter()
                        .map(|v| v.as_f64().unwrap_or(0.0) as f32)
                        .collect()
                })
                .unwrap_or_default();
            (idx, vec)
        })
        .collect();
    indexed.sort_unstable_by_key(|(i, _)| *i);

    Ok(indexed.into_iter().map(|(_, v)| v).collect())
}

/// Convenience wrapper that embeds a single text.
/// Prefer `embed_texts` when embedding multiple facts in one turn.
pub async fn embed_text(state: &AppState, text: &str) -> Result<Vec<f32>> {
    embed_texts(state, &[text])
        .await?
        .into_iter()
        .next()
        .ok_or_else(|| anyhow!("Embedding API returned empty data array"))
}
