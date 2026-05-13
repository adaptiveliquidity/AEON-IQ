use anyhow::Result;
use crate::{embeddings::embed_text, models::Memory, AppState};
use super::store;

/// Embed `query`, run a vector similarity search, and return memories whose
/// cosine distance is below `threshold` (lower = more similar).
pub async fn retrieve_relevant(
    state: &AppState,
    agent_id: &str,
    query: &str,
    limit: i64,
    threshold: f64,
) -> Result<Vec<Memory>> {
    let embedding = embed_text(state, query).await?;
    let rows = store::search_memories(state, agent_id, &embedding, limit).await?;

    Ok(rows
        .into_iter()
        .filter(|r| r.distance.unwrap_or(1.0) < threshold)
        .map(|r| Memory {
            id: r.id,
            agent_id: r.agent_id,
            session_id: r.session_id,
            content: r.content,
            memory_type: r.memory_type,
            confidence: r.confidence,
            created_at: r.created_at,
            source_turn: r.source_turn,
        })
        .collect())
}

/// Build the memory context string that is injected as the first system message.
pub fn build_injection(memories: &[Memory], working_summary: Option<&str>) -> String {
    let mut sections = Vec::new();

    if let Some(s) = working_summary {
        if !s.trim().is_empty() {
            sections.push(format!("## Session Summary\n{}", s));
        }
    }

    if !memories.is_empty() {
        let lines: Vec<String> = memories
            .iter()
            .enumerate()
            .map(|(i, m)| format!("[M{}] ({}) {}", i + 1, m.memory_type, m.content))
            .collect();
        sections.push(format!("## Relevant Memories\n{}", lines.join("\n")));
    }

    if sections.is_empty() {
        return String::new();
    }

    format!(
        "<memory_context>\n{}\n</memory_context>\n\n\
         Use the above context when answering. Do not mention it explicitly unless the user asks.",
        sections.join("\n\n")
    )
}
