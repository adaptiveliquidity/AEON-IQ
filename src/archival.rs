//! LTM Archival Job — Phase B
//!
//! Runs on a configurable interval (default 24 h).  For each agent that has
//! at least `archival_min_memories` L2 memories older than `archival_min_age_days`
//! with zero retrieval hits, it:
//!
//! 1. Fetches up to 50 such memories.
//! 2. Asks the extractor LLM to compress them into 3-5 essential facts.
//! 3. Stores the compressed facts as L3 (`tier = 'L3'`, lower confidence).
//! 4. Tombstones the originals (sets archived_at = NOW()) — they are retained
//!    for audit/lineage but excluded from all retrieval queries.
//!
//! The job is started via `tokio::spawn(archival::run_job(state))` in `main.rs`
//! and only runs when `ARCHIVAL_INTERVAL_HOURS > 0`.

use anyhow::Result;
use tracing::{error, info, warn};

use crate::{embeddings::embed_texts, memory::store, AppState};

// ── Entry point ───────────────────────────────────────────────────────────────

pub async fn run_job(state: AppState) {
    let interval_secs = state.config.archival_interval_hours * 3600;
    let mut ticker =
        tokio::time::interval(std::time::Duration::from_secs(interval_secs));

    // Skip the first immediate tick so the job waits a full interval before
    // its first run (prevents stampede on startup).
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);
    ticker.tick().await; // consume the first immediate tick

    loop {
        ticker.tick().await;
        info!("LTM archival cycle starting");
        if let Err(e) = run_cycle(&state).await {
            error!("LTM archival cycle error: {:#}", e);
            state
                .metrics
                .archival_total
                .with_label_values(&["error"])
                .inc();
        } else {
            state
                .metrics
                .archival_total
                .with_label_values(&["ok"])
                .inc();
        }
    }
}

// ── One full cycle ────────────────────────────────────────────────────────────

async fn run_cycle(state: &AppState) -> Result<()> {
    let min_age = state.config.archival_min_age_days as i64;
    let min_count = state.config.archival_min_memories;

    let agent_ids = store::agents_with_archivable_memories(state, min_age).await?;
    info!("Archival candidates: {} agents", agent_ids.len());

    for agent_id in agent_ids {
        if let Err(e) = archive_agent(state, &agent_id, min_age, min_count).await {
            warn!(agent_id = %agent_id, "Per-agent archival failed: {}", e);
        }
    }
    Ok(())
}

// ── Per-agent compaction ──────────────────────────────────────────────────────

async fn archive_agent(
    state: &AppState,
    agent_id: &str,
    min_age_days: i64,
    min_memories: usize,
) -> Result<()> {
    let candidates = store::fetch_archivable_memories(state, agent_id, min_age_days, 50).await?;

    if candidates.len() < min_memories {
        return Ok(());
    }

    let count = candidates.len();
    info!(agent_id = %agent_id, count, "Compacting L2 → L3");

    // ── Ask the LLM to compress facts ─────────────────────────────────────────
    let numbered: Vec<String> = candidates
        .iter()
        .enumerate()
        .map(|(i, (_, c))| format!("{}. {}", i + 1, c))
        .collect();

    let prompt = format!(
        "Compress these {} memory facts into 3-5 concise, high-signal points. \
         Preserve the most important concrete information. \
         Return a JSON object with a single key \"facts\" containing an array of strings. \
         Example: {{\"facts\": [\"Alex is building NovaPay\", \"NovaPay does cross-border payments\"]}}\n\n{}",
        candidates.len(),
        numbered.join("\n")
    );

    let api_key = state.config.openai_api_key.as_deref().unwrap_or("no-key");
    let resp = state
        .http_client
        .post(format!("{}/v1/chat/completions", state.config.extractor_base_url))
        .header("Authorization", format!("Bearer {}", api_key))
        .header("Content-Type", "application/json")
        .json(&serde_json::json!({
            "model": state.config.extractor_model,
            "messages": [{"role": "user", "content": prompt}],
            "temperature": 0.1,
            "response_format": {"type": "json_object"}
        }))
        .send()
        .await?;

    if !resp.status().is_success() {
        return Err(anyhow::anyhow!("Compaction LLM returned {}", resp.status()));
    }

    let body: serde_json::Value = resp.json().await?;
    let raw = body["choices"][0]["message"]["content"]
        .as_str()
        .ok_or_else(|| anyhow::anyhow!("No compaction content"))?;

    let compressed = parse_compressed_facts(raw);
    if compressed.is_empty() {
        warn!(agent_id = %agent_id, "No compressed facts returned — skipping deletion");
        return Ok(());
    }

    // ── Create versioned batch record before mutating any rows ───────────────
    let stored_count = compressed.len();
    let batch_id = store::create_archival_batch(
        state,
        agent_id,
        count as i32,
        stored_count as i32,
    )
    .await?;

    // ── Batch-embed all compressed facts, then store as L3 ───────────────────
    let fact_refs: Vec<&str> = compressed.iter().map(|s| s.as_str()).collect();
    match embed_texts(state, &fact_refs).await {
        Ok(embeddings) => {
            for (fact, emb) in compressed.iter().zip(embeddings) {
                if let Err(e) = store::store_memory_with_tier(
                    state,
                    agent_id,
                    None,
                    fact,
                    "semantic",
                    0.7,
                    emb,
                    None,
                    "L3",
                    "inferred",
                    0.5_f32,
                    "extractor",
                    Some(batch_id),
                )
                .await
                {
                    warn!(agent_id = %agent_id, "Failed to store L3 fact: {}", e);
                }
            }
        }
        Err(e) => {
            warn!(agent_id = %agent_id, "Batch embedding failed for L3 facts: {}", e);
            return Ok(());
        }
    }

    // ── Tombstone originals and tag them with the batch ───────────────────────
    let ids: Vec<uuid::Uuid> = candidates.iter().map(|(id, _)| *id).collect();
    let tombstoned = store::tombstone_memories_with_batch(state, &ids, batch_id).await?;

    state
        .metrics
        .archival_compacted
        .observe(tombstoned as f64);

    info!(
        agent_id = %agent_id,
        batch = %batch_id,
        original = count,
        compressed = stored_count,
        tombstoned,
        "L2→L3 compaction complete"
    );

    Ok(())
}

// ── Helpers ───────────────────────────────────────────────────────────────────

fn parse_compressed_facts(raw: &str) -> Vec<String> {
    // Try {"facts": [...]} first, then a bare array.
    if let Ok(v) = serde_json::from_str::<serde_json::Value>(raw) {
        let arr = v["facts"].as_array().or_else(|| v.as_array());
        if let Some(arr) = arr {
            return arr
                .iter()
                .filter_map(|x| x.as_str().map(|s| s.to_string()))
                .collect();
        }
    }
    // Last resort: try as bare JSON array of strings.
    serde_json::from_str::<Vec<String>>(raw).unwrap_or_default()
}
