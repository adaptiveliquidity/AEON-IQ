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
2. **Memory retrieval** — embed the last user message; `search_memories_filtered` runs a two-CTE pgvector query; inject results as a `<retrieved_memories>` system message at position 0
3. **Forward upstream** — translate request format via `Provider::build_request`, add any provider-specific headers
4. **Tee response** — for OpenAI/Gemini: stream chunks to client while capturing the full body; for Anthropic: buffer entirely (Anthropic's wire format differs), then re-emit as synthetic OpenAI SSE/JSON so callers using the OpenAI SDK need zero changes
5. **Background extraction** — `tokio::spawn(extract_and_store(...))`: calls the extractor LLM to pull structured facts, batch-embeds them in one API call, stores to Postgres

### Memory tiers

| Tier | Table | Description |
|------|-------|-------------|
| L1 | `working_memory` | Per-session rolling summary; updated every turn |
| L2 | `memories` (tier='L2') | Individual extracted facts; default tier |
| L3 | `memories` (tier='L3') | Compressed archival facts; confidence capped at 0.7 |

Archival job (`src/archival.rs`) runs on `ARCHIVAL_INTERVAL_HOURS` schedule: compacts stale zero-access L2 facts into L3 via LLM compression, then tombstones (sets `archived_at = NOW()`) originals. All queries filter `AND archived_at IS NULL` — nothing is hard-deleted.

### Decay-weighted retrieval

`search_memories_filtered` uses a two-CTE SQL pattern:
- `base` — cosine distance + `days_stale` (days since `last_accessed_at`, or `created_at` if never accessed)
- `ranked` — `adjusted_dist = cosine_dist * (1 + MEMORY_DECAY_RATE * days_stale)`

When `MEMORY_DECAY_RATE=0.0` (default) the formula collapses to pure cosine similarity.

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

- **Kernel management API** (`/api/v1/*`): `X-Management-Key` or `Authorization: Bearer` header; no-op when `MANAGEMENT_API_KEY` is unset (local dev)
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
| `RATE_LIMIT_RPM` | `0` | Per-agent request cap; 0 = disabled |
| `MANAGEMENT_API_KEY` | unset | Unauthenticated if unset |
| `EMBEDDING_DIMENSION` | `1536` | Must match `vector(N)` in schema |

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

### Observability

Prometheus metrics at `GET /metrics` (text format 0.0.4). Key counters/histograms: `memoryos_requests_total`, `memoryos_extraction_total{status=ok|error|low_confidence}`, `memoryos_injection_total{result=hit|miss}`, `memoryos_rate_limited_total`. Grafana dashboard provisioned via `docker/grafana/provisioning/`.
