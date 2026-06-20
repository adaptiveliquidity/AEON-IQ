# HNSW Index Maintenance Runbook

> AEON-IQ / MemoryOS Kernel — keeping vector retrieval fast and accurate as memory churns.
> NexusIQ Phase-P readiness item (4 of 4).

## The index

`memories.embedding` is searched via a single pgvector **HNSW** index, created in `migrations/0001_initial.sql`:

```sql
CREATE INDEX IF NOT EXISTS idx_memories_hnsw
    ON memories USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);
```

- **Op class** `vector_cosine_ops` — matches the cosine-distance operator `embedding <=> <query>` used by retrieval (`src/memory/store.rs`, `retrieval.rs`, `conflicts.rs`).
- **Dimension** `vector(1536)` (text-embedding-3-small; 384 for bge-small). Build params `m = 16`, `ef_construction = 64`.
- **Query-time recall knob** `hnsw.ef_search` — **not currently set by the kernel**, so it uses pgvector's default of **40**.

## Why it drifts

`memories` is high-churn: extraction inserts L2 facts, the LTM job compacts L2→L3, AMP soft-evicts, and the management API hard-deletes. HNSW is a navigable-graph index, so:

1. **Tombstones stay in the index.** Archival and soft-eviction are *soft* deletes (`archived_at` / `soft_evicted` / `status`); the row and its vector remain. Retrieval filters them **in SQL** (`WHERE archived_at IS NULL AND soft_evicted = FALSE AND status = 'active'`) — *after* the ANN search. So as tombstoned/L3 rows accumulate, the ANN returns more candidates that are then discarded, and **effective live-recall drops** even though the index still "works."
2. **Hard deletes leave dead graph elements.** `DELETE /memories/:id` and agent cascade deletes remove rows; pgvector marks the graph elements deleted (skipped at query time but still traversed) until `VACUUM`/`REINDEX` reclaims them → latency creep + bloat.
3. **Graph quality degrades** under sustained high insert volume far from the original `ef_construction` build.

## What to monitor

| Signal | Source | Watch for |
|---|---|---|
| Retrieval latency p50/p95/p99 | `memory_retrieval_logs.latency_ms`; Prometheus | p99 rising > ~25% over a rolling baseline |
| Injection hit rate | `memoryos_injection_total{result}` (hit/miss) | sustained hit-rate decline (recall proxy) |
| Dead-tuple ratio on `memories` | `pg_stat_user_tables` | `n_dead_tup / n_live_tup` > ~0.2 |
| Index vs live rows | query below | index entries >> live (`archived_at IS NULL`) rows |
| Index size | `pg_relation_size('idx_memories_hnsw')` | unbounded growth vs retained live set |

Health snapshot:

```sql
SELECT
  (SELECT count(*) FROM memories)                          AS total_rows,
  (SELECT count(*) FROM memories WHERE archived_at IS NULL
                                   AND soft_evicted = FALSE
                                   AND status = 'active')   AS live_rows,
  pg_size_pretty(pg_relation_size('idx_memories_hnsw'))     AS hnsw_size,
  s.n_live_tup, s.n_dead_tup,
  round(s.n_dead_tup::numeric / NULLIF(s.n_live_tup, 0), 3) AS dead_ratio,
  s.last_autovacuum, s.last_autoanalyze
FROM pg_stat_user_tables s
WHERE s.relname = 'memories';
```

## Maintenance actions

### 1. VACUUM / ANALYZE (routine)
Keep autovacuum on (default). For hard-delete-heavy workloads, schedule a manual pass on the worker:

```sql
VACUUM (ANALYZE) memories;
```
`VACUUM` reclaims deleted HNSW graph elements and dead heap tuples; `ANALYZE` refreshes planner stats. If churn is high, tighten autovacuum for this table:

```sql
ALTER TABLE memories SET (autovacuum_vacuum_scale_factor = 0.05,
                          autovacuum_analyze_scale_factor = 0.02);
```

### 2. REINDEX (periodic rebuild)
Rebuild the graph to drop accumulated deleted elements and restore quality. **Always `CONCURRENTLY`** so retrieval keeps serving:

```sql
SET maintenance_work_mem = '2GB';   -- speeds the rebuild; size to host RAM
REINDEX INDEX CONCURRENTLY idx_memories_hnsw;
```
Cadence: when `dead_ratio > 0.2`, index size >> live rows, or p99 drifts > ~25% — otherwise a standing monthly/quarterly job sized to churn. Run on the **`worker`** role (`MEMORYOS_ROLE=worker`, see the proxy/worker split) so the proxy stays low-jitter.

`CONCURRENTLY` caveats: cannot run inside a transaction block; on failure it may leave an `INVALID` index — detect + clean up:

```sql
SELECT indexrelid::regclass FROM pg_index WHERE NOT indisvalid;
-- if idx_memories_hnsw is invalid:
DROP INDEX CONCURRENTLY idx_memories_hnsw;   -- then recreate / REINDEX again
```

### 3. Tune `hnsw.ef_search` (recall vs latency)
Default **40**. Raise to recover recall — *especially* here, because the post-ANN SQL filter discards tombstoned rows, so a higher `ef_search` keeps enough **live** candidates in the result:

```sql
ALTER DATABASE memoryos SET hnsw.ef_search = 100;  -- global, new sessions
SET hnsw.ef_search = 100;                          -- session
SET LOCAL hnsw.ef_search = 100;                    -- single transaction
```
Higher `ef_search` → better recall, more latency. Start 64–100; raise on heavily-tombstoned agents with low hit-rate.

> **Follow-up (code):** the kernel does not set `ef_search` today. A future change could expose `HNSW_EF_SEARCH` and `SET LOCAL` it on the retrieval transaction (optionally scaled by an agent's tombstone ratio).

### 4. Rebuild with new build params (`m` / `ef_construction`)
`m` and `ef_construction` are fixed at build time; changing them needs a rebuild. Higher = better recall, slower build, more memory:

```sql
SET maintenance_work_mem = '2GB';
CREATE INDEX CONCURRENTLY idx_memories_hnsw_v2
    ON memories USING hnsw (embedding vector_cosine_ops)
    WITH (m = 24, ef_construction = 128);
BEGIN;
  DROP INDEX idx_memories_hnsw;
  ALTER INDEX idx_memories_hnsw_v2 RENAME TO idx_memories_hnsw;
COMMIT;
```
Guidance: `m` 16→24–32 and `ef_construction` 64→128–200 for larger/recall-sensitive deployments. Changing `EMBEDDING_DIMENSION` (e.g. 1536→384) also requires rebuilding this index and `ALTER TABLE memories ALTER COLUMN embedding TYPE vector(384)`.

### 5. Tombstone retention (optional, reduces index pressure)
Soft-deleted rows never leave the index. For agents with large tombstoned/L3 history, enforce retention: export (`GET /api/v1/agents/:id/export`), then **hard-delete** aged tombstones so they leave the graph, then `REINDEX`. Irreversible — it drops time-travel history for purged rows.

## Cadence summary

| Trigger | Action |
|---|---|
| Always | autovacuum on; monitor latency + injection hit-rate |
| `dead_ratio > 0.2` or monthly/quarterly | `REINDEX INDEX CONCURRENTLY` (worker role) |
| Low recall on tombstoned agents | raise `hnsw.ef_search` |
| Larger corpus / recall-sensitive | rebuild with higher `m` / `ef_construction` |
| Unbounded tombstone growth | retention policy → hard-delete aged → reindex |
| Embedding model/dim change | rebuild index (+ column type) |

## Notes
- Run heavy maintenance (`REINDEX`, large `VACUUM`) on the **`worker`** process so the proxy hot path is unaffected.
- `REINDEX CONCURRENTLY` and `VACUUM` cannot run inside a transaction block.
- Size `maintenance_work_mem` to the host; too little makes HNSW builds slow.
- pgvector ≥ 0.5 is required for HNSW (the `pgvector/pgvector:pg16` image satisfies this).

*Phase-P readiness runbook. Pairs with the proxy/worker split (`MEMORYOS_ROLE`) and the extraction outbox.*
