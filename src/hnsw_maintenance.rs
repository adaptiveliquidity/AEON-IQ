use std::time::Instant;

use anyhow::{anyhow, Context, Result};
use sqlx::PgConnection;
use tracing::{error, info, warn};

use crate::AppState;

const HNSW_INDEX_NAME: &str = "idx_memories_hnsw";
const HNSW_TABLE_NAME: &str = "memories";

#[derive(Debug, Clone, Copy)]
struct HnswStats {
    live_rows: i64,
    dead_rows: i64,
    index_size_bytes: i64,
}

pub async fn run_job(state: AppState) {
    let interval = std::time::Duration::from_secs(
        state
            .config
            .hnsw_maintenance_interval_hours
            .saturating_mul(60 * 60),
    );

    info!(
        interval_hours = state.config.hnsw_maintenance_interval_hours,
        reindex_enabled = state.config.hnsw_maintenance_reindex_enabled,
        vacuum_enabled = state.config.hnsw_maintenance_vacuum_enabled,
        vacuum_analyze = state.config.hnsw_maintenance_vacuum_analyze,
        lock_id = state.config.hnsw_maintenance_advisory_lock_id,
        "HNSW maintenance worker started"
    );

    let mut ticker = tokio::time::interval(interval);
    ticker.set_missed_tick_behavior(tokio::time::MissedTickBehavior::Skip);

    // Match other background jobs: wait one full interval before first pass.
    ticker.tick().await;
    loop {
        ticker.tick().await;

        let started = Instant::now();
        let outcome = run_cycle(&state).await;
        let elapsed_secs = started.elapsed().as_secs_f64();

        state
            .metrics
            .hnsw_maintenance_duration_seconds
            .observe(elapsed_secs);

        match &outcome {
            Ok(Some(rows_reclaimed)) => {
                state
                    .metrics
                    .hnsw_maintenance_total
                    .with_label_values(&["ok"])
                    .inc();
                state
                    .metrics
                    .hnsw_dead_tuples_reclaimed
                    .set(*rows_reclaimed as f64);
            }
            Ok(None) => {
                state
                    .metrics
                    .hnsw_maintenance_total
                    .with_label_values(&["skipped"])
                    .inc();
            }
            Err(_) => {
                state
                    .metrics
                    .hnsw_maintenance_total
                    .with_label_values(&["error"])
                    .inc();
            }
        }

        if let Err(e) = outcome {
            error!("HNSW maintenance cycle failed: {:#}", e);
        }
    }
}

async fn run_cycle(state: &AppState) -> Result<Option<i64>> {
    let mut conn = state
        .db
        .acquire()
        .await
        .context("acquire database connection for HNSW maintenance")?;

    let lock_id = state.config.hnsw_maintenance_advisory_lock_id;
    let lock_acquired: bool = sqlx::query_scalar("SELECT pg_try_advisory_lock($1)")
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await
        .context("acquire advisory lock for HNSW maintenance")?;

    if !lock_acquired {
        info!(
            lock_id,
            "HNSW maintenance skipped: another worker currently holds advisory lock"
        );
        return Ok(None);
    }

    let run_result = run_maintenance_cycle(state, &mut conn).await;
    let unlock_result = release_lock(&mut conn, lock_id).await;

    if let Err(e) = &unlock_result {
        warn!(lock_id, "Could not release HNSW advisory lock: {:#}", e);
    }

    run_result
}

async fn run_maintenance_cycle(state: &AppState, conn: &mut PgConnection) -> Result<Option<i64>> {
    let before = gather_stats(conn).await?;
    info!(
        live_rows = before.live_rows,
        dead_rows = before.dead_rows,
        index_size_bytes = before.index_size_bytes,
        "HNSW maintenance pre-stats"
    );

    if let Some(maintenance_work_mem) = state.config.hnsw_maintenance_work_mem.as_deref() {
        sqlx::query("SET maintenance_work_mem = $1")
            .bind(maintenance_work_mem)
            .execute(&mut *conn)
            .await
            .context("set maintenance_work_mem for HNSW maintenance")?;
    }

    if state.config.hnsw_maintenance_reindex_enabled {
        sqlx::query("REINDEX INDEX CONCURRENTLY idx_memories_hnsw")
            .execute(&mut *conn)
            .await
            .context("reindex idx_memories_hnsw")?;
    }

    if state.config.hnsw_maintenance_vacuum_enabled {
        if state.config.hnsw_maintenance_vacuum_analyze {
            sqlx::query("VACUUM (ANALYZE) memories")
                .execute(&mut *conn)
                .await
                .context("vacuum memories table with analyze")?;
        } else {
            sqlx::query("VACUUM memories")
                .execute(&mut *conn)
                .await
                .context("vacuum memories table")?;
        }
    } else {
        info!("HNSW maintenance vacuum step is disabled by configuration");
    }

    let after = gather_stats(conn).await?;
    let dead_tuples_reclaimed = (before.dead_rows - after.dead_rows).max(0);
    let live_row_delta = after.live_rows - before.live_rows;
    let index_size_delta = after.index_size_bytes - before.index_size_bytes;

    info!(
        table = HNSW_TABLE_NAME,
        index = HNSW_INDEX_NAME,
        dead_rows_before = before.dead_rows,
        dead_rows_after = after.dead_rows,
        dead_tuples_reclaimed,
        live_rows_before = before.live_rows,
        live_rows_after = after.live_rows,
        live_rows_delta = live_row_delta,
        index_size_bytes_before = before.index_size_bytes,
        index_size_bytes_after = after.index_size_bytes,
        index_size_bytes_delta = index_size_delta,
        "HNSW maintenance complete"
    );
    Ok(Some(dead_tuples_reclaimed))
}

async fn gather_stats(conn: &mut PgConnection) -> Result<HnswStats> {
    let (live_rows, dead_rows): (i64, i64) = sqlx::query_as(
        "SELECT n_live_tup::bigint, n_dead_tup::bigint
         FROM pg_stat_user_tables
         WHERE relname = $1",
    )
    .bind(HNSW_TABLE_NAME)
    .fetch_one(&mut *conn)
    .await
    .context("read pg_stat_user_tables stats for memories")?;

    let index_size_bytes: i64 = sqlx::query_scalar("SELECT pg_relation_size($1::regclass)::bigint")
        .bind(HNSW_INDEX_NAME)
        .fetch_one(&mut *conn)
        .await
        .context("read idx_memories_hnsw relation size")?;

    Ok(HnswStats {
        live_rows,
        dead_rows,
        index_size_bytes,
    })
}

async fn release_lock(conn: &mut PgConnection, lock_id: i64) -> Result<()> {
    let lock_released: bool = sqlx::query_scalar("SELECT pg_advisory_unlock($1)")
        .bind(lock_id)
        .fetch_one(&mut *conn)
        .await
        .context("release advisory lock for HNSW maintenance")?;
    if !lock_released {
        return Err(anyhow!("advisory lock {lock_id} was not released"));
    }
    Ok(())
}
