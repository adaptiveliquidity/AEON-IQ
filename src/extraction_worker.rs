use std::{future::Future, time::Duration};

use anyhow::{Context, Result};
use chrono::{DateTime, Duration as ChronoDuration, Utc};
use sqlx::PgPool;
use tracing::{error, info, warn};
use uuid::Uuid;

use crate::{
    config::ExtractionOutboxConfig,
    memory::extraction::{run_extraction_payload, ExtractionPayload},
    AppState,
};

const CLAIM_BATCH_SIZE: i64 = 10;

#[derive(Debug, Clone, sqlx::FromRow)]
#[allow(dead_code)]
pub(crate) struct ExtractionJob {
    pub id: Uuid,
    pub agent_id: String,
    pub session_id: Option<String>,
    pub payload: serde_json::Value,
    pub status: String,
    pub attempts: i32,
    pub next_attempt_at: DateTime<Utc>,
    pub last_error: Option<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

pub async fn run_job(state: AppState, config: ExtractionOutboxConfig) {
    let poll_secs = config.worker_poll_secs.max(1);
    info!(
        poll_secs,
        max_attempts = config.max_attempts,
        backoff_base_secs = config.backoff_base_secs,
        "Extraction outbox worker started"
    );

    loop {
        if let Err(e) = drain_due_jobs(&state, &config).await {
            error!("Extraction outbox worker cycle failed: {:#}", e);
        }
        tokio::time::sleep(Duration::from_secs(poll_secs)).await;
    }
}

async fn drain_due_jobs(state: &AppState, config: &ExtractionOutboxConfig) -> Result<usize> {
    let state_for_jobs = state.clone();

    drain_due_jobs_with_executor(
        &state.db,
        CLAIM_BATCH_SIZE,
        config.max_attempts,
        config.backoff_base_secs,
        move |job| {
            let state = state_for_jobs.clone();
            async move { execute_extraction_job(&state, &job).await }
        },
    )
    .await
}

pub(crate) async fn drain_due_jobs_with_executor<F, Fut>(
    pool: &PgPool,
    batch_size: i64,
    max_attempts: u32,
    backoff_base_secs: u64,
    mut executor: F,
) -> Result<usize>
where
    F: FnMut(ExtractionJob) -> Fut,
    Fut: Future<Output = Result<()>>,
{
    let jobs = claim_due_jobs(pool, batch_size).await?;
    let processed = jobs.len();

    for job in jobs {
        let result = executor(job.clone()).await;
        if let Err(e) = finish_job(pool, &job, result, max_attempts, backoff_base_secs).await {
            warn!(job_id = %job.id, "Could not update extraction job status: {:#}", e);
        }
    }

    Ok(processed)
}

pub(crate) async fn claim_due_jobs(pool: &PgPool, batch_size: i64) -> Result<Vec<ExtractionJob>> {
    let jobs = sqlx::query_as::<_, ExtractionJob>(
        "UPDATE extraction_jobs
         SET status = 'in_progress',
             attempts = attempts + 1,
             updated_at = NOW()
         WHERE id IN (
             SELECT id
             FROM extraction_jobs
             WHERE status = 'pending'
               AND next_attempt_at <= NOW()
             ORDER BY next_attempt_at
             LIMIT $1
             FOR UPDATE SKIP LOCKED
         )
         RETURNING id, agent_id, session_id, payload, status, attempts,
                   next_attempt_at, last_error, created_at, updated_at",
    )
    .bind(batch_size)
    .fetch_all(pool)
    .await?;

    Ok(jobs)
}

async fn execute_extraction_job(state: &AppState, job: &ExtractionJob) -> Result<()> {
    let payload: ExtractionPayload = serde_json::from_value(job.payload.clone())
        .with_context(|| format!("invalid extraction payload for job {}", job.id))?;
    let session_id = job
        .session_id
        .as_deref()
        .with_context(|| format!("extraction job {} is missing session_id", job.id))?;

    // Extraction is at-least-once; retries rely on store_memory's DEDUP_THRESHOLD
    // near-duplicate check rather than introducing a second deduplication system.
    match run_extraction_payload(state, &job.agent_id, session_id, &payload).await {
        Ok(()) => Ok(()),
        Err(e) => {
            state
                .metrics
                .extraction_total
                .with_label_values(&["error"])
                .inc();
            Err(e)
        }
    }
}

async fn finish_job(
    pool: &PgPool,
    job: &ExtractionJob,
    result: Result<()>,
    max_attempts: u32,
    backoff_base_secs: u64,
) -> Result<()> {
    match result {
        Ok(()) => mark_done(pool, job.id).await,
        Err(e) => {
            let last_error = format!("{:#}", e);
            if (job.attempts as u32) < max_attempts {
                let next_attempt_at =
                    Utc::now() + backoff_after_attempt(job.attempts, backoff_base_secs);
                mark_retry(pool, job.id, next_attempt_at, &last_error).await
            } else {
                mark_failed(pool, job.id, &last_error).await
            }
        }
    }
}

async fn mark_done(pool: &PgPool, id: Uuid) -> Result<()> {
    sqlx::query(
        "UPDATE extraction_jobs
         SET status = 'done',
             last_error = NULL,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_retry(
    pool: &PgPool,
    id: Uuid,
    next_attempt_at: DateTime<Utc>,
    last_error: &str,
) -> Result<()> {
    sqlx::query(
        "UPDATE extraction_jobs
         SET status = 'pending',
             next_attempt_at = $2,
             last_error = $3,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(next_attempt_at)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

async fn mark_failed(pool: &PgPool, id: Uuid, last_error: &str) -> Result<()> {
    sqlx::query(
        "UPDATE extraction_jobs
         SET status = 'failed',
             last_error = $2,
             updated_at = NOW()
         WHERE id = $1",
    )
    .bind(id)
    .bind(last_error)
    .execute(pool)
    .await?;
    Ok(())
}

fn backoff_after_attempt(attempts: i32, base_secs: u64) -> ChronoDuration {
    let exponent = attempts.saturating_sub(1).min(30) as u32;
    let multiplier = 1_u64 << exponent;
    let seconds = base_secs.saturating_mul(multiplier).min(i64::MAX as u64);
    ChronoDuration::seconds(seconds as i64)
}

#[cfg(test)]
mod memory {
    pub mod store {
        pub mod tests {
            use anyhow::Result;
            use chrono::{DateTime, Utc};
            use serde_json::Value;

            use crate::{
                extraction_worker::{claim_due_jobs, drain_due_jobs_with_executor},
                memory::extraction::{enqueue_extraction_job, ExtractionPayload},
                models::Message,
            };

            fn payload() -> ExtractionPayload {
                ExtractionPayload::new(
                    vec![Message {
                        role: "user".to_string(),
                        content: "My name is Alex.".into(),
                        name: None,
                    }],
                    "Nice to meet you, Alex.".to_string(),
                    1,
                    Some(0.9),
                )
            }

            async fn job_status(
                pool: &sqlx::PgPool,
                id: uuid::Uuid,
            ) -> (String, i32, DateTime<Utc>, Option<String>) {
                sqlx::query_as(
                    "SELECT status, attempts, next_attempt_at, last_error
                     FROM extraction_jobs
                     WHERE id = $1",
                )
                .bind(id)
                .fetch_one(pool)
                .await
                .unwrap()
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn enqueue_inserts_pending_job(pool: sqlx::PgPool) {
                let id = enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload())
                    .await
                    .unwrap();

                let (status, attempts, session_id, row_payload): (
                    String,
                    i32,
                    Option<String>,
                    Value,
                ) = sqlx::query_as(
                    "SELECT status, attempts, session_id, payload
                     FROM extraction_jobs
                     WHERE id = $1",
                )
                .bind(id)
                .fetch_one(&pool)
                .await
                .unwrap();

                assert_eq!(status, "pending");
                assert_eq!(attempts, 0);
                assert_eq!(session_id.as_deref(), Some("session-a"));
                assert_eq!(row_payload["assistant_content"], "Nice to meet you, Alex.");
                assert_eq!(row_payload["turn_number"], 1);
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn claim_marks_due_job_in_progress_once(pool: sqlx::PgPool) {
                let id = enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload())
                    .await
                    .unwrap();

                let (left, right) =
                    tokio::join!(claim_due_jobs(&pool, 10), claim_due_jobs(&pool, 10));
                let left = left.unwrap();
                let right = right.unwrap();
                assert_eq!(left.len() + right.len(), 1);

                let claimed = left.into_iter().chain(right).next().unwrap();
                assert_eq!(claimed.id, id);
                assert_eq!(claimed.status, "in_progress");
                assert_eq!(claimed.attempts, 1);

                let second_claim = claim_due_jobs(&pool, 10).await.unwrap();
                assert!(second_claim.is_empty());

                let (status, attempts, _, _) = job_status(&pool, id).await;
                assert_eq!(status, "in_progress");
                assert_eq!(attempts, 1);
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn successful_drain_marks_job_done(pool: sqlx::PgPool) {
                let id = enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload())
                    .await
                    .unwrap();

                let processed = drain_due_jobs_with_executor(&pool, 10, 5, 2, |_job| async {
                    Ok::<(), anyhow::Error>(())
                })
                .await
                .unwrap();

                assert_eq!(processed, 1);
                let (status, attempts, _, last_error) = job_status(&pool, id).await;
                assert_eq!(status, "done");
                assert_eq!(attempts, 1);
                assert!(last_error.is_none());
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn failed_drain_retries_with_future_next_attempt(pool: sqlx::PgPool) {
                let id = enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload())
                    .await
                    .unwrap();
                let before = Utc::now();

                let processed = drain_due_jobs_with_executor(&pool, 10, 5, 60, |_job| async {
                    Err::<(), anyhow::Error>(anyhow::anyhow!("extractor unavailable"))
                })
                .await
                .unwrap();

                assert_eq!(processed, 1);
                let (status, attempts, next_attempt_at, last_error) = job_status(&pool, id).await;
                assert_eq!(status, "pending");
                assert_eq!(attempts, 1);
                assert!(next_attempt_at > before);
                assert!(last_error.unwrap().contains("extractor unavailable"));
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn failed_drain_marks_failed_after_max_attempts(pool: sqlx::PgPool) {
                let id = enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload())
                    .await
                    .unwrap();

                let processed = drain_due_jobs_with_executor(&pool, 10, 1, 2, |_job| async {
                    Err::<(), anyhow::Error>(anyhow::anyhow!("extractor unavailable"))
                })
                .await
                .unwrap();

                assert_eq!(processed, 1);
                let (status, attempts, _, last_error) = job_status(&pool, id).await;
                assert_eq!(status, "failed");
                assert_eq!(attempts, 1);
                assert!(last_error.unwrap().contains("extractor unavailable"));
            }

            #[sqlx::test(migrations = "./migrations")]
            async fn future_job_is_not_claimed(pool: sqlx::PgPool) -> Result<()> {
                let id =
                    enqueue_extraction_job(&pool, "agent-a", Some("session-a"), &payload()).await?;

                sqlx::query(
                    "UPDATE extraction_jobs
                     SET next_attempt_at = NOW() + INTERVAL '1 hour'
                     WHERE id = $1",
                )
                .bind(id)
                .execute(&pool)
                .await?;

                let jobs = claim_due_jobs(&pool, 10).await?;
                assert!(jobs.is_empty());

                let (status, attempts, _, _) = job_status(&pool, id).await;
                assert_eq!(status, "pending");
                assert_eq!(attempts, 0);
                Ok(())
            }
        }
    }
}
