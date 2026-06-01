# CLAUDE.md

This file provides guidance to Claude Code (claude.ai/code) when working with code in this repository.

## Commands

### Rust kernel
```bash
cargo check                        # fast type-check (no DB needed)
cargo build                        # debug build
cargo build --release              # production build

# Unit tests only (no database required)
cargo test -- --skip memory::store::tests

# All tests including DB integration tests (requires DATABASE_URL → pgvector Postgres)
# Run `docker compose up postgres` first, then:
cargo test
```

The 4 integration tests in `src/memory/store.rs` (marked `#[sqlx::test]`) each create an isolated test database, run all migrations, execute the test, then drop the database. They need a Postgres instance with pgvector installed and a user with `CREATEDB` privilege — the `docker compose up postgres` service satisfies this. They will panic with "DATABASE_URL must be set" if no database is available, which is expected in environments without Postgres.

### Full stack (Docker)
```bash
docker compose up --build          # build + start all services
docker compose up --build memoryos # rebuild kernel only
docker compose logs -f memoryos    # tail kernel logs
```

### Dashboard (Next.js)
```bash
cd dashboard
npm install
npm run dev        # dev server on :3000
npm run build      # production build
npm run lint       # ESLint
```

### Smoke test
```bash
OPENAI_API_KEY=sk-... python test_memory.py   # requires running kernel on :8080
```

## Architecture

### Request lifecycle

Every `POST /v1/chat/completions` goes through `src/proxy.rs`:

1. **Auth & rate limit** — extract `x-agent-id` (required), `x-session-id` (auto-generated if absent); check per-agent token bucket (`src/rate_limit.rs`)
2. **Memory retrieval** — embed the last user message; `search_memories_filtered` runs a two-CTE pgvector query; when `AMP_ENABLED` or `RMK_ENABLED`, a `RetrievalAugmenter` re-ranks results by subtracting co-access graph bonuses from the cosine distance; when `GRAPH_RETRIEVAL_ENABLED=true`, a one-hop entity graph walk further augments the set; inject results as a `<retrieved_memories>` system message at position 0; background-record pairwise co-access edges for all retrieved memories
3. **Forward upstream** — translate request format via `Provider::build_request`, add any provider-specific headers
4. **Tee response** — for OpenAI/Gemini: stream chunks to client while capturing the full body; for Anthropic: buffer entirely (Anthropic's wire format differs), then re-emit as synthetic OpenAI SSE/JSON so callers using the OpenAI SDK need zero changes
5. **Background extraction** — `tokio::spawn(extract_and_store(...))`: calls the extractor LLM to pull structured facts, batch-embeds them in one API call, stores to Postgres

### Memory tiers

| Tier | Table | Description |
|------|-------|-------------|
| L1 | `working_memory` | Per-session rolling summary + structured state (JSONB); updated every turn |
| L2 | `memories` (tier='L2') | Individual extracted facts; default tier |
| L3 | `memories` (tier='L3') | Compressed archival facts; confidence capped at 0.7 |

L1 structured state (`working_memory.state` JSONB, added Phase 4.3):
```json
{ "summary": "...", "active_entities": ["NovaPay"], "current_goal": "...", "open_questions": ["..."] }
```
When present, `build_injection` renders each field as a labelled section (`[ACTIVE_ENTITIES]`, `[CURRENT_GOAL]`, `[OPEN_QUESTIONS]`) instead of a single `[SESSION_SUMMARY]` block. The plain `summary` column is kept for backward compatibility.

Archival job (`src/archival.rs`) runs on `ARCHIVAL_INTERVAL_HOURS` schedule: compacts stale zero-access L2 facts into L3 via LLM compression, then tombstones (sets `archived_at = NOW()`) originals. All queries filter `AND archived_at IS NULL` — nothing is hard-deleted.

Each compaction run creates an **archival batch** (`archival_batches` table) that links the tombstoned L2 sources and the new L3 facts via `memories.archival_batch_id`. This enables atomic batch-level restore: `POST /api/v1/archival/batches/:batch_id/restore` un-tombstones L2 memories and re-tombstones the L3 facts that replaced them, then sets `batch.status = 'restored'`. A restored batch cannot be restored again (idempotency guard). If the embedding step fails after the batch record is created, the batch is marked `status = 'failed'` (migration 0010).

### Decay-weighted retrieval

`search_memories_filtered` uses a two-CTE SQL pattern:
- `base` — cosine distance + `days_stale` (days since `last_accessed_at`, or `created_at` if never accessed)
- `ranked` — three-factor adjusted distance:

```
adjusted_dist = cosine_dist
    × (1 + MEMORY_DECAY_RATE × days_stale)
    × (1 + IMPORTANCE_BOOST_FACTOR × (1 − importance_score))
```

When `MEMORY_DECAY_RATE=0.0` and `IMPORTANCE_BOOST_FACTOR=0.0` (defaults) the formula collapses to pure cosine similarity.

### Importance scoring

Each memory has an `importance_score` (0.0–1.0) and `importance_source` assigned at extraction time. The score comes from one of three signals, in priority order:

1. **`user_stated`** — `x-memory-importance` request header (e.g. `x-memory-importance: 0.95`) — caller override, highest priority
2. **`agent_marked`** — `<important>…</important>` XML tags in assistant responses — score floored at 0.9
3. **`extractor`** — LLM-assigned score using a four-tier rubric:
   - 1.0 = critical/permanent (identity, goals, compliance rules)
   - 0.8–0.99 = high business value (key decisions, product names)
   - 0.5–0.79 = standard episodic detail or preference
   - 0.0–0.49 = trivial / conversational filler

**Archival protection**: memories with `importance_score >= 0.9` are excluded from the L2→L3 compaction job — they are never automatically archived regardless of age or access count.

**Refresh-on-read**: every retrieval increments `importance_score` by `IMPORTANCE_REFRESH_BOOST` (default 0.05, capped at 1.0), implementing a spacing-effect memory reinforcement signal.

### Provider abstraction

`src/providers.rs` contains the `Provider` enum (OpenAI / Anthropic / Gemini). Key methods: `completions_url`, `build_request`, `parse_buffered`, `parse_streaming`, `synthesize_json`, `synthesize_sse`. Set `UPSTREAM_PROVIDER` to select.

Anthropic-specific: `build_anthropic_body` lifts system messages to the top-level `system` field and merges consecutive same-role messages (Anthropic requires alternating roles). The response is always buffered server-side and re-emitted as OpenAI-format SSE.

Embeddings use a separate endpoint (`EMBEDDING_BASE_URL`, defaults to `UPSTREAM_BASE_URL`) so Anthropic users can point embeddings at OpenAI independently.

### Provenance and confidence

Facts extracted from conversation are tagged with provenance:
- `user_stated` — capped at 0.95 confidence
- `assistant_derived` — capped at 0.70 (may be hallucinated)
- `inferred` — capped at 0.50

The extraction prompt (`EXTRACTION_SYSTEM_PROMPT` in `src/memory/extraction.rs`) requires a `cited_line` for every fact and rejects injection-like content.

### Database

Migrations run automatically at startup via `sqlx::migrate!("./migrations")`. The kernel uses `sqlx::query` (non-macro form) throughout to avoid compile-time DB dependency — never use `sqlx::query!` macros.

`QueryBuilder` is used for dynamic filter composition (filters on `memory_type`, `session_id`, etc.). The vector embedding is always bound exactly once inside a CTE to avoid double-binding.

### Authentication

- **Kernel management API** (`/api/v1/*`): `X-Management-Key` or `Authorization: Bearer` header.
  - Key comparison uses **constant-time equality** (`subtle::ConstantTimeEq`) to prevent timing side-channel attacks.
  - **Startup guard**: the server refuses to start unless `MANAGEMENT_API_KEY` is set OR `ALLOW_UNAUTH_MANAGEMENT=true` is explicitly provided. This prevents accidental unauthenticated exposure when the env var is forgotten in a deployment.
  - Development shortcut: `ALLOW_UNAUTH_MANAGEMENT=true` (logs a prominent warning at startup).
- **Dashboard**: NextAuth v5 (Auth.js) with Credentials provider; JWT sessions; `session.user.agentId` is derived from email as `email.toLowerCase().replace(/[@.+]/g, "-")`; non-admins are scoped to their own `agentId`

### Key config variables

| Variable | Default | Notes |
|----------|---------|-------|
| `UPSTREAM_PROVIDER` | `openai` | `openai` \| `anthropic` \| `gemini` |
| `UPSTREAM_BASE_URL` | `https://api.openai.com` | LLM base URL |
| `EMBEDDING_BASE_URL` | `UPSTREAM_BASE_URL` | Override for embeddings only |
| `EXTRACTOR_BASE_URL` | `https://api.openai.com` | Override for extraction LLM |
| `RETRIEVAL_THRESHOLD` | `0.80` | Cosine distance upper bound |
| `MEMORY_DECAY_RATE` | `0.0` | Per-day staleness penalty; 0 = disabled |
| `IMPORTANCE_BOOST_FACTOR` | `0.0` | Importance weight in retrieval; 0 = disabled |
| `IMPORTANCE_REFRESH_BOOST` | `0.05` | Per-retrieval importance bump; 0 = disabled |
| `RATE_LIMIT_RPM` | `0` | Per-agent request cap; 0 = disabled |
| `MANAGEMENT_API_KEY` | unset | Required unless `ALLOW_UNAUTH_MANAGEMENT=true`; protects `/api/v1/*` |
| `ALLOW_UNAUTH_MANAGEMENT` | `false` | Must be `true` when no management key is set (dev only; logs warning) |
| `MAX_BODY_BYTES` | `10485760` | Request body size limit in bytes (10 MiB default); HTTP 413 if exceeded |
| `EMBEDDING_DIMENSION` | `1536` | Must match `vector(N)` in schema |
| `GRAPH_RETRIEVAL_ENABLED` | `false` | Enable graph-walk augmentation during retrieval |
| `DB_MAX_CONNECTIONS` | `20` | PgPool max connections |
| `DB_ACQUIRE_TIMEOUT_SECS` | `5` | Seconds to wait for a pool connection before error |
| `DB_IDLE_TIMEOUT_SECS` | `300` | Seconds before idle connections are reclaimed |
| `DEDUP_THRESHOLD` | `0.05` | Cosine distance below which an insert is skipped as a near-duplicate; 0 = disabled |
| `CONFLICT_DETECTION_ENABLED` | `false` | Enable async LLM-based contradiction detection on each L2 insert |
| `AMP_ENABLED` | `false` | Enable Adaptive Memory Pressure (co-access graph bonuses, pressure scoring) |
| `RMK_ENABLED` | `false` | Enable Reflexive Memory Kernel (learned policy θ; implies AMP co-access recording) |

To switch embedding model to bge-small (384 dims): change `EMBEDDING_MODEL`, `EMBEDDING_DIMENSION=384`, and update `vector(1536)` → `vector(384)` in `migrations/0001_initial.sql`.

### AppState

`AppState` in `src/main.rs` is `Clone` (backed by `Arc`s) and threaded through every handler via Axum's `State` extractor:

```
AppState {
    config:       Arc<Config>,
    db:           PgPool,
    http_client:  reqwest::Client,
    metrics:      Arc<Metrics>,
    provider:     Provider,          // Copy
    rate_limiter: Arc<RateLimiter>,
}
```

### Management API summary

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/agents` | List all agents |
| DELETE | `/api/v1/agents/:id` | Delete agent and all its data (cascade) |
| GET | `/api/v1/agents/:id/memories` | Paginated live memories |
| POST | `/api/v1/agents/:id/memories` | Create memory manually |
| PATCH | `/api/v1/memories/:id` | Update memory content (re-embeds) |
| GET | `/api/v1/agents/:id/memories/archived` | Tombstoned memories |
| GET | `/api/v1/agents/:id/archival/batches` | Archival batch history |
| POST | `/api/v1/agents/:id/archival/trigger` | Manually trigger L2→L3 compaction |
| GET | `/api/v1/agents/:id/sessions` | List sessions with turn counts |
| GET | `/api/v1/agents/:id/sessions/:sid` | Session detail + working memory |
| DELETE | `/api/v1/agents/:id/sessions/:sid` | Delete session working memory |
| POST | `/api/v1/memories/search` | Semantic search |
| DELETE | `/api/v1/memories/:id` | Hard-delete a memory |
| POST | `/api/v1/memories/:id/restore` | Restore individual tombstoned memory |
| POST | `/api/v1/archival/batches/:id/restore` | Restore entire batch (L2 back, L3 tombstoned) |
| POST | `/api/v1/agents/:id/memories/bulk` | Bulk archive or delete memories by filter |
| GET | `/api/v1/agents/:id/conflicts` | List unresolved (or all) memory conflicts |
| POST | `/api/v1/conflicts/:id/resolve` | Resolve a conflict (keep_a/keep_b/keep_both/dismissed) |
| GET | `/api/v1/agents/:id/export` | Export all live memories as NDJSON (no embeddings) |
| POST | `/api/v1/agents/:id/import` | Import NDJSON; re-embeds each memory, runs dedup check |
| GET | `/api/v1/stats` | Agent + memory counts |

### Observability

Prometheus metrics at `GET /metrics` (text format 0.0.4). Key counters/histograms: `memoryos_requests_total`, `memoryos_extraction_total{status=ok|error|low_confidence}`, `memoryos_injection_total{result=hit|miss}`, `memoryos_rate_limited_total`. Grafana dashboard provisioned via `docker/grafana/provisioning/`.

### Knowledge graph (Phase 4)

`entities` and `memory_graph` tables store named entities and subject-predicate-object triples extracted by the MMU. Phase 4 additions:

- **`memory_entity_links`** (migration 0007) — join table mapping `memory_id → entity_id`. Populated by `extraction.rs` after each turn: every stored memory is linked to every entity extracted in the same turn.
- **Graph-walk retrieval** (`GRAPH_RETRIEVAL_ENABLED=true`) — `retrieve_relevant` matches entity names from the query against `entities`, walks `memory_graph` one hop, then fetches memories linked to those related entities via `memory_entity_links`. Results are merged after the primary vector set.
- **Entity disambiguation** (migration 0008, `fuzzystrmatch`) — `upsert_entity` checks `levenshtein(LOWER(name), LOWER(new)) <= 2` before inserting. Near-duplicates (e.g. "Alex" / "Alexander") are merged into the canonical (existing) entity name.

### Adaptive Memory Pressure (AMP)

`AMP_ENABLED=true` activates a closed-loop system that keeps active memory count near a configurable target and builds a co-access pheromone graph.

**Components** (`src/memory/amp/`):

| File | Role |
|------|------|
| `pressure.rs` | Stateless formula: `pressure = a·days_stale + b·(1 − utility_ema)`, clamped [0, 1] |
| `pi_controller.rs` | PI controller that drives the soft-eviction threshold |
| `co_access.rs` | `CoAccessGraph` — records pairwise co-occurrence edges in `co_access_edges`; provides `get_neighbor_weight_sum` for retrieval bonus |
| `augmenter.rs` | `RetrievalAugmenter` — subtracts `graph_bonus_weight × neighbour_weight_sum` from cosine distance to re-rank retrieved memories |
| `utility.rs` | EMA tracker for per-memory retrieval frequency |
| `config.rs` | `AmpConfig` with defaults for all sub-params |

**New DB columns** added to `memories` (migration 0014):
- `utility_ema DOUBLE PRECISION` — EMA of retrieval frequency
- `pressure DOUBLE PRECISION` — computed pressure score
- `soft_evicted BOOLEAN` — soft-eviction flag
- `soft_evicted_at TIMESTAMPTZ`

**`co_access_edges` table** (migration 0015): undirected, normalised to `(min_uuid, max_uuid)` order; `weight` accumulates on conflict, capped at `max_edge_weight`. A nightly decay pass (via `rmk_worker::run_co_access_decay_job`) multiplies all weights by `(1 − decay_per_day)` and prunes below `min_edge_weight`.

**Retrieval wiring** (`src/memory/retrieval.rs`): when AMP or RMK is enabled and the result set is non-empty, a `RetrievalAugmenter` is instantiated from `AppState` config. It calls `get_neighbor_weight_sum` for each candidate (5 indexed DB lookups), subtracts the scaled bonus from the cosine distance, and re-sorts before building `Memory` structs.

### Reflexive Memory Kernel (RMK)

`RMK_ENABLED=true` replaces static config thresholds with a **per-agent learned policy vector** θ = `[pressure_a, pressure_b, kp, ki, graph_bonus_weight, retrieval_threshold]`.

**Components** (`src/memory/rmk/`):

| File | Role |
|------|------|
| `policy.rs` | `PolicyParams` struct — θ dimensions with AMP-matching defaults |
| `reward.rs` | `RewardModel`: `R = task·success + 0.5·token_savings + precision@5 − 0.1·eviction_cost` |
| `meta_learner.rs` | ε-greedy exploration: perturbs each param ±10% of its bounded range with probability `epsilon` (default 0.1); hill-climbing acceptance keeps perturbation only if reward ≥ last reward |
| `adapter.rs` | `RmkAdapter::apply` — maps θ back to `PressureParams`, `ControllerParams`, `CoAccessParams` |
| `buffer.rs` | In-memory `EpisodeBuffer` ring-buffer (unused in DB-backed path; reserved for phase 2) |
| `store.rs` | DB CRUD: `get_latest_policy`, `insert_policy`, `insert_episode`, `get_recent_episode_rewards`, `count_episodes`, `list_all_agent_ids_with_episodes` |
| `config.rs` | `RmkConfig` with epsilon, buffer_size, min_episodes_before_update, update_cooldown_secs, reward_weights |

**DB tables** (migrations 0017–0018):
- `rmk_policies` — versioned θ rows per agent; indexed by `agent_id`; latest row wins
- `rmk_episodes` — per-turn metrics log (task_success, token_savings, retrieval_precision, eviction_cost, reward, policy_id FK)

**Per-request flow** (`src/proxy.rs`):
1. Fetch latest policy from `rmk_policies` for the agent; use `policy.retrieval_threshold` (falls back to static `RETRIEVAL_THRESHOLD` if no policy exists yet)
2. Capture `memories_retrieved` and `rmk_policy_id` after retrieval
3. After the upstream response is dispatched, background-spawn `rmk_store::insert_episode` with computed metrics and reward

**Background worker** (`src/rmk_worker.rs`, spawned from `main.rs`):
- `run_policy_update_job` — sleeps for `update_cooldown_secs` (default 3600 s); for each agent with ≥ `min_episodes_before_update` (default 20) episodes, applies `MetaLearner::suggest_explore()` and persists the (possibly perturbed) policy
- `run_co_access_decay_job` — runs once per day; calls `CoAccessGraph::decay_all()` to apply weight decay and prune stale edges

Both workers start when `RMK_ENABLED=true`; `run_co_access_decay_job` also starts when `AMP_ENABLED=true` without RMK.

### CI/CD (Phase 3)

GitHub Actions workflows in `.github/workflows/`:
- **`ci.yml`** — runs on every push/PR to `main`: Rust lint (`clippy`, `rustfmt`), unit tests (no DB), integration tests (Postgres+pgvector service container), dashboard lint (`npm run lint`).
- **`publish.yml`** — Docker publish workflow; builds and pushes the kernel image to the registry on release tags.

### MCP Server (Phase 3)

A TypeScript Model Context Protocol server lives in `mcp-server/`. It exposes AEON-IQ memory operations as MCP tools so any MCP-compatible AI assistant can read/write memories without direct HTTP calls.

```bash
cd mcp-server
npm install
npm run build
```

Key tools exposed: `retrieve_memories`, `store_memory`, `list_agents`, `get_stats`.

### Client SDKs (Phase 3)

`sdks/python/` — Python SDK wrapping the management API and proxy endpoint. Install with `pip install -e sdks/python`.

`sdks/typescript/` — TypeScript/Node SDK. Install with `npm install` inside `sdks/typescript/`.
