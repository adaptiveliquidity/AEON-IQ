use std::time::Duration;

use tracing::{info, warn};

use crate::{
    memory::{
        amp::{
            co_access::CoAccessGraph,
            pi_controller::PIController,
            pressure::PressureManager,
        },
        rmk::{meta_learner::MetaLearner, policy::PolicyParams, store},
    },
    AppState,
};

/// Periodically runs the RMK policy update cycle.
///
/// For every agent that has accumulated at least `min_episodes_before_update`
/// episodes since RMK was enabled, the meta-learner generates a new candidate
/// policy via ε-greedy exploration and persists it to `rmk_policies`.  The
/// next retrieval for that agent will pick up the new threshold automatically.
pub async fn run_policy_update_job(state: AppState) {
    let cooldown = state.config.rmk_config.update_cooldown_secs;
    let min_episodes = state.config.rmk_config.min_episodes_before_update;
    let epsilon = state.config.rmk_config.epsilon;

    info!(
        cooldown_secs = cooldown,
        min_episodes,
        epsilon,
        "RMK policy-update worker started"
    );

    loop {
        tokio::time::sleep(Duration::from_secs(cooldown)).await;

        let agents = match store::list_all_agent_ids_with_episodes(&state.db).await {
            Ok(a) => a,
            Err(e) => {
                warn!("RMK worker: failed to list agents: {}", e);
                continue;
            }
        };

        for agent_id in agents {
            if let Err(e) =
                update_policy_for_agent(&state, &agent_id, min_episodes, epsilon).await
            {
                warn!(agent_id = %agent_id, "RMK: policy update error: {}", e);
            }
        }
    }
}

async fn update_policy_for_agent(
    state: &AppState,
    agent_id: &str,
    min_episodes: usize,
    epsilon: f64,
) -> anyhow::Result<()> {
    let count = store::count_episodes(&state.db, agent_id).await?;
    if count < min_episodes as i64 {
        return Ok(());
    }

    // Compute mean reward from up to 50 recent episodes.
    let rewards = store::get_recent_episode_rewards(&state.db, agent_id, 50).await?;
    if rewards.is_empty() {
        return Ok(());
    }
    let mean_reward = rewards.iter().sum::<f64>() / rewards.len() as f64;

    // Load the current policy (or seed with defaults on first run).
    let current = match store::get_latest_policy(&state.db, agent_id).await? {
        Some((_, p)) => p,
        None => PolicyParams::default(),
    };

    // ε-greedy: generate a (possibly perturbed) candidate policy.
    let learner = MetaLearner::new(epsilon, current);
    let candidate = learner.suggest_explore();

    store::insert_policy(&state.db, agent_id, &candidate).await?;
    info!(agent_id = %agent_id, mean_reward, "RMK: policy updated");
    Ok(())
}

/// Periodically computes per-memory pressure scores and applies soft-eviction.
///
/// For each agent that has active memories, a fresh PIController is driven
/// by the gap between current and target active count.  Memories whose
/// computed pressure exceeds `threshold_high` are soft-evicted; soft-evicted
/// memories whose pressure has fallen below `threshold_low` are restored.
///
/// Runs every 5 minutes when AMP or RMK is enabled.
pub async fn run_pressure_sweep_job(state: AppState) {
    info!("AMP pressure sweep worker started (interval=5min)");

    loop {
        tokio::time::sleep(Duration::from_secs(5 * 60)).await;

        // Fetch all distinct agent IDs that have at least one active memory.
        let agents: Vec<String> = match sqlx::query_scalar(
            "SELECT DISTINCT agent_id FROM memories WHERE archived_at IS NULL",
        )
        .fetch_all(&state.db)
        .await
        {
            Ok(a) => a,
            Err(e) => {
                warn!("Pressure sweep: failed to list agents: {}", e);
                continue;
            }
        };

        for agent_id in agents {
            if let Err(e) = run_pressure_sweep_for_agent(&state, &agent_id).await {
                warn!(agent_id = %agent_id, "Pressure sweep error: {}", e);
            }
        }
    }
}

async fn run_pressure_sweep_for_agent(state: &AppState, agent_id: &str) -> anyhow::Result<()> {
    let target = state.config.amp_config.target_active_count;
    let pressure_params = state.config.amp_config.pressure_params.clone();
    let controller_params = state.config.amp_config.controller_params.clone();
    let min_age_seconds = controller_params.min_age_seconds;

    // Count active (non-archived, non-soft-evicted) memories for this agent.
    let current_count: i64 = sqlx::query_scalar(
        "SELECT COUNT(*) FROM memories
         WHERE agent_id = $1 AND archived_at IS NULL AND soft_evicted = FALSE",
    )
    .bind(agent_id)
    .fetch_one(&state.db)
    .await?;

    // Fresh PI controller per sweep — acceptable for Phase 1; integral_error
    // will re-converge within a few cycles.
    let mut pi = PIController::new(controller_params);
    let (threshold_high, threshold_low) =
        pi.update(current_count as u64, target, 1.0);

    // Fetch all non-archived memories (including currently soft-evicted ones so we
    // can restore them) with the pressure-relevant columns.
    let rows = sqlx::query(
        "SELECT id, last_accessed_at, created_at, utility_ema, soft_evicted
         FROM memories
         WHERE agent_id = $1 AND archived_at IS NULL",
    )
    .bind(agent_id)
    .fetch_all(&state.db)
    .await?;

    let pm = PressureManager::new(pressure_params);
    let now = chrono::Utc::now();

    let mut to_evict: Vec<uuid::Uuid> = Vec::new();
    let mut to_restore: Vec<uuid::Uuid> = Vec::new();
    let mut pressure_updates: Vec<(uuid::Uuid, f64)> = Vec::new();

    for row in rows {
        use sqlx::Row;
        let id: uuid::Uuid = row.get("id");
        let last_accessed: Option<chrono::DateTime<chrono::Utc>> = row.get("last_accessed_at");
        let created_at: chrono::DateTime<chrono::Utc> = row.get("created_at");
        let utility_ema: f64 = row.get("utility_ema");
        let soft_evicted: bool = row.get("soft_evicted");

        let pressure = pm.compute_pressure(last_accessed, created_at, utility_ema, now);
        pressure_updates.push((id, pressure));

        if soft_evicted {
            if PressureManager::should_restore(pressure, threshold_low) {
                to_restore.push(id);
            }
        } else if PressureManager::should_soft_evict(
            pressure,
            threshold_high,
            created_at,
            min_age_seconds,
            now,
        ) {
            to_evict.push(id);
        }
    }

    // Batch-update pressure scores.
    for (id, pressure) in &pressure_updates {
        let _ = sqlx::query("UPDATE memories SET pressure = $1 WHERE id = $2")
            .bind(pressure)
            .bind(id)
            .execute(&state.db)
            .await;
    }

    // Soft-evict.
    if !to_evict.is_empty() {
        sqlx::query(
            "UPDATE memories
             SET soft_evicted = TRUE, soft_evicted_at = NOW()
             WHERE id = ANY($1)",
        )
        .bind(&to_evict)
        .execute(&state.db)
        .await?;
        info!(agent_id = %agent_id, count = to_evict.len(), "AMP: soft-evicted memories");
    }

    // Restore.
    if !to_restore.is_empty() {
        sqlx::query(
            "UPDATE memories
             SET soft_evicted = FALSE, soft_evicted_at = NULL
             WHERE id = ANY($1)",
        )
        .bind(&to_restore)
        .execute(&state.db)
        .await?;
        info!(agent_id = %agent_id, count = to_restore.len(), "AMP: restored soft-evicted memories");
    }

    Ok(())
}

/// Periodically decays co-access edge weights and prunes stale edges.
///
/// Runs once per day.  Decay prevents old co-access signals from dominating
/// the bonus score as conversation topics shift.
pub async fn run_co_access_decay_job(state: AppState) {
    info!("Co-access decay job started (interval=24h)");

    loop {
        tokio::time::sleep(Duration::from_secs(24 * 60 * 60)).await;

        let graph = CoAccessGraph::new(
            state.db.clone(),
            state.config.amp_config.co_access_params.clone(),
        );
        match graph.decay_all().await {
            Ok(()) => info!("Co-access decay pass completed"),
            Err(e) => warn!("Co-access decay failed: {}", e),
        }
    }
}
