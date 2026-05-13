use anyhow::Result;
use tracing::{error, info, warn};

use crate::{
    embeddings::embed_text,
    models::{ExtractionResult, Message},
    AppState,
};
use super::store;

/// The system prompt fed to the extractor LLM.
pub const EXTRACTION_SYSTEM_PROMPT: &str = r#"You are MemoryOS MMU v2. Analyze the full conversation turn.
Output ONLY valid JSON. No explanations.

{
  "facts": ["fact1 (cited: line 3 in transcript)"],
  "entities": [{"name": "Project Apollo", "type": "project", "confidence": 0.95}],
  "relations": [{"subject": "user", "predicate": "working_on", "object": "Project Apollo"}],
  "updated_summary": "Short executive summary (max 200 tokens)",
  "memory_type": "episodic|semantic|procedural",
  "confidence_low": false
}

If any fact cannot be directly cited from the transcript, set confidence_low: true."#;

/// Entry point called from a `tokio::spawn`. Never panics; logs errors instead.
pub async fn extract_and_store(
    state: AppState,
    agent_id: String,
    session_id: String,
    messages: Vec<Message>,
    assistant_content: String,
    turn_number: i32,
) {
    if let Err(e) = run_extraction(
        &state,
        &agent_id,
        &session_id,
        &messages,
        &assistant_content,
        turn_number,
    )
    .await
    {
        error!(agent_id = %agent_id, "Memory extraction failed: {:#}", e);
    }
}

async fn run_extraction(
    state: &AppState,
    agent_id: &str,
    session_id: &str,
    messages: &[Message],
    assistant_content: &str,
    turn_number: i32,
) -> Result<()> {
    // Build numbered transcript
    let mut lines: Vec<String> = messages
        .iter()
        .enumerate()
        .map(|(i, m)| format!("Line {}: [{}]: {}", i + 1, m.role, m.content))
        .collect();
    lines.push(format!("Line {}: [assistant]: {}", messages.len() + 1, assistant_content));
    let transcript = lines.join("\n");

    info!(agent_id = %agent_id, turn = turn_number, "Running MMU extraction");

    // Call extractor LLM
    let api_key = state.config.openai_api_key.as_deref().unwrap_or("no-key");
    let payload = serde_json::json!({
        "model": state.config.extractor_model,
        "messages": [
            {"role": "system", "content": EXTRACTION_SYSTEM_PROMPT},
            {
                "role": "user",
                "content": format!("Extract memories from this conversation:\n\n{}", transcript)
            }
        ],
        "temperature": 0.1,
        "response_format": {"type": "json_object"}
    });

    let resp = state
        .http_client
        .post(format!("{}/v1/chat/completions", state.config.extractor_base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&payload)
        .send()
        .await?;

    if !resp.status().is_success() {
        let status = resp.status();
        let body = resp.text().await.unwrap_or_default();
        return Err(anyhow::anyhow!("Extractor API {}: {}", status, body));
    }

    let body: serde_json::Value = resp.json().await?;
    let raw = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No content in extractor response: {:?}", body))?;

    let extraction: ExtractionResult = match serde_json::from_str(raw) {
        Ok(e) => e,
        Err(err) => {
            warn!(agent_id = %agent_id, "Could not parse extraction JSON: {} — raw: {}", err, &raw[..raw.len().min(500)]);
            return Ok(());
        }
    };

    if extraction.confidence_low {
        info!(agent_id = %agent_id, "Low-confidence extraction — skipping storage");
        return Ok(());
    }

    // Store facts as L2 memories with embeddings
    for fact in &extraction.facts {
        match embed_text(state, fact).await {
            Ok(emb) => {
                if let Err(e) = store::store_memory(
                    state,
                    agent_id,
                    Some(session_id),
                    fact,
                    &extraction.memory_type,
                    0.9,
                    emb,
                    Some(turn_number),
                )
                .await
                {
                    warn!(agent_id = %agent_id, "Failed to store fact: {}", e);
                }
            }
            Err(e) => warn!(agent_id = %agent_id, "Failed to embed fact: {}", e),
        }
    }

    // Store entities
    for entity in &extraction.entities {
        if let Err(e) =
            store::upsert_entity(state, agent_id, &entity.name, &entity.entity_type, entity.confidence)
                .await
        {
            warn!(agent_id = %agent_id, "Failed to store entity: {}", e);
        }
    }

    // Store relations
    for rel in &extraction.relations {
        if let Err(e) =
            store::insert_relation(state, agent_id, &rel.subject, &rel.predicate, &rel.object).await
        {
            warn!(agent_id = %agent_id, "Failed to store relation: {}", e);
        }
    }

    // Update L1 working memory
    if let Err(e) =
        store::upsert_working_memory(state, agent_id, session_id, &extraction.updated_summary).await
    {
        warn!(agent_id = %agent_id, "Failed to update working memory: {}", e);
    }

    info!(
        agent_id = %agent_id,
        facts = extraction.facts.len(),
        entities = extraction.entities.len(),
        relations = extraction.relations.len(),
        "MMU extraction complete"
    );

    Ok(())
}

// ── Response content parsers ──────────────────────────────────────────────────

/// Extracts the assistant text from a streaming SSE response body.
pub fn parse_sse_content(data: &[u8]) -> String {
    let text = String::from_utf8_lossy(data);
    let mut content = String::new();
    for line in text.lines() {
        if let Some(payload) = line.strip_prefix("data: ") {
            if payload == "[DONE]" {
                continue;
            }
            if let Ok(json) = serde_json::from_str::<serde_json::Value>(payload) {
                if let Some(c) = json["choices"][0]["delta"]["content"].as_str() {
                    content.push_str(c);
                }
            }
        }
    }
    content
}

/// Extracts the assistant text from a non-streaming JSON response body.
pub fn parse_json_content(data: &[u8]) -> String {
    serde_json::from_slice::<serde_json::Value>(data)
        .ok()
        .and_then(|v| {
            v["choices"][0]["message"]["content"]
                .as_str()
                .map(|s| s.to_string())
        })
        .unwrap_or_default()
}
