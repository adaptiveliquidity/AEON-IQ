use std::time::Duration;

use tracing::{info, warn};

use crate::{
    memory::{
        amp::co_access::CoAccessGraph,
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
