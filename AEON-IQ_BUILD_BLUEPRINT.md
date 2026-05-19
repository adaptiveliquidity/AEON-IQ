# AEON-IQ Build Blueprint

> **Generated:** 2026-05-19 · **Branch:** `claude/memoryos-kernel-mvp-1B8Rg`
> **Scope:** Everything from current state → production-ready open-source → SaaS
> **Principle:** Each phase produces a shippable state. No phase depends on a later one.

---

## How to read this

Each phase has a **gate** — the condition that must be true before starting the next phase. Within each phase, items are ordered by dependency (top items unblock bottom items). Items marked `[parallel]` can be worked on simultaneously.

---

## Phase 0 — Stabilize & Harden

**Goal:** Fix everything that's fragile in the existing codebase so building on top doesn't create hidden failures.

**Why before anything else:** Every feature in Phases 1–5 touches `store.rs`, `proxy.rs`, or the DB pool. If these have latent issues, every future feature inherits them.

### 0.1 — Connection pool configuration `[~30 min]`

**File:** `src/db.rs`

The current code uses `PgPool::connect()` with sqlx defaults (10 connections, no timeouts). Under concurrent agent load this will either exhaust connections or leak idle ones.

```rust
// Replace PgPool::connect(url) with:
use sqlx::postgres::PgPoolOptions;

PgPoolOptions::new()
    .max_connections(env_or("DB_MAX_CONNECTIONS", 20))
    .acquire_timeout(Duration::from_secs(5))
    .idle_timeout(Duration::from_secs(300))
    .connect(database_url)
    .await?
```

Add `DB_MAX_CONNECTIONS`, `DB_ACQUIRE_TIMEOUT_SECS`, `DB_IDLE_TIMEOUT_SECS` to `config.rs` and `docker-compose.yml`.

### 0.2 — Audit `.unwrap()` calls in production paths `[~1 hr]` `[parallel]`

**Files:** `src/proxy.rs`, `src/providers.rs`

Grep for `.unwrap()` outside of `#[cfg(test)]` blocks. The context transfer doc confirms these are on response builders and JSON serialization — theoretically safe but should be `.expect("reason")` at minimum, or proper `?` propagation where the function signature allows it.

**Action:** Replace every non-test `.unwrap()` with either `.expect("descriptive reason")` or `?` + `map_err`. Zero behavior change, just crash diagnostics.

### 0.3 — Archival batch `failed` status `[~20 min]` `[parallel]`

**File:** `migrations/0007_archival_failed_status.sql`, `src/archival.rs`

Currently if the LLM compaction call fails after the batch record is created, the batch stays `status = 'completed'` with 0 L3 facts. Add a `'failed'` variant.

```sql
-- 0007_archival_failed_status.sql
ALTER TABLE archival_batches DROP CONSTRAINT IF EXISTS archival_batches_status_check;
ALTER TABLE archival_batches ADD CONSTRAINT archival_batches_status_check
    CHECK (status IN ('completed', 'restored', 'failed'));
```

In `archival.rs::archive_agent`, wrap the embedding+store block in error handling that sets `status = 'failed'` on the batch if anything after `create_archival_batch` fails.

### 0.4 — Resolve orphaned `sessions` table `[~30 min]` `[parallel]`

**Files:** `migrations/0001_initial.sql`, `src/memory/store.rs`

The `sessions` table exists in the schema but nothing reads or writes it. Two options:

- **Option A (recommended):** Leave it, but wire it up in Phase 1 when building the working memory API. The table is already well-structured.
- **Option B:** Drop it via migration if the schema cleanliness bothers you. But you'll recreate it in Phase 1 anyway.

**Decision for Claude Code:** Leave it. Add a `-- NOTE: sessions table is scaffolded; wired in Phase 1` comment to `0001_initial.sql`.

### 0.5 — Smoke-test archival job against a live LLM `[~1 hr]`

**Action:** Manually run the archival compaction with a real API key.

```bash
# Seed an agent with 15+ memories older than 1 day
# Set ARCHIVAL_MIN_AGE_DAYS=0 ARCHIVAL_MIN_MEMORIES=5 ARCHIVAL_INTERVAL_HOURS=0
# Call the cycle directly or trigger via short interval
```

Document what happens. Fix any parsing issues in `parse_compressed_facts`. This has never been tested end-to-end against a real model.

### Phase 0 Gate

✅ Pool configured with explicit limits
✅ No bare `.unwrap()` in non-test code
✅ Archival batches can record failure
✅ Archival job has been run against a real LLM at least once
✅ All existing `cargo test -- --skip memory::store::tests` pass

---

## Phase 1 — Complete the MVP Surface

**Goal:** Make the management API and dashboard feature-complete for a first user who self-hosts and manages one agent.

**Why now:** Without these, the dashboard is demo-ware. PATCH is table-stakes for any data management UI. Working memory inspection is the #1 thing operators ask for. Pagination is a UX cliff at 50+ memories.

### 1.1 — Memory PATCH endpoint `[~2 hrs]`

**Files:** `src/api.rs`, `src/memory/store.rs`, `src/embeddings.rs`

Add `PATCH /api/v1/memories/:id` that accepts `{ "content": "new text" }`.

The store function:
1. Fetch the existing memory row (verify it exists, get `agent_id`)
2. Embed the new content via `embed_text()`
3. `UPDATE memories SET content = $1, embedding = $2, updated_at = NOW() WHERE id = $3`
4. Return the updated row

**Schema change:** Add `updated_at TIMESTAMPTZ` column to `memories` table (migration 0008). Default `NOW()`, backfill existing rows to `created_at`.

**Dashboard:** Add an "Edit" button to the memory card in the explorer. Opens an inline text editor. Saves via the PATCH route.

### 1.2 — Working memory API `[~2 hrs]`

**Files:** `src/api.rs`, `src/memory/store.rs`

New endpoints:

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/agents/:id/sessions` | List all working_memory rows for agent (session_id, summary preview, turn_count, updated_at) |
| GET | `/api/v1/agents/:id/sessions/:session_id` | Full L1 summary for a session |
| DELETE | `/api/v1/agents/:id/sessions/:session_id` | Clear working memory for a session |

Wire the `sessions` table: on `upsert_working_memory`, also upsert into `sessions` with `turn_count` and `ended_at = NULL`.

**Dashboard:** Add a "Sessions" tab in the memory explorer showing active sessions and their L1 summaries.

### 1.3 — Dashboard pagination `[~2 hrs]` `[parallel with 1.1/1.2]`

**Files:** `src/api.rs` (backend), `dashboard/src/app/memory-explorer/client.tsx` (frontend)

Backend: `list_memories` already accepts `limit`. Add `offset` parameter (default 0). Return total count in response envelope: `{ "memories": [...], "total": 123, "offset": 0, "limit": 50 }`.

Frontend: Add page controls (Previous / Next) or infinite scroll with intersection observer. Show "Showing 1–50 of 1,234".

### 1.4 — Manual archival trigger `[~1 hr]`

**File:** `src/api.rs`, `src/archival.rs`

Add `POST /api/v1/agents/:id/archival/trigger`.

Extract `archive_agent` to be callable from both the cron job and the API handler. Return the batch ID synchronously (the compaction is fast enough for a request — it's one LLM call + a few DB writes).

```json
// Response
{ "batch_id": "uuid", "source_count": 47, "l3_count": 5, "status": "completed" }
```

### 1.5 — Delete agent endpoint `[~1 hr]` `[parallel]`

**File:** `src/api.rs`, `src/memory/store.rs`

Add `DELETE /api/v1/agents/:id`.

Cascade: delete all `memories`, `working_memory`, `entities`, `memory_graph`, `archival_batches`, `sessions`, `audit_logs` for this agent, then the agent row itself. Use a DB transaction.

**Dashboard:** Add a "Delete Agent" button with confirmation modal on the overview page.

### Phase 1 Gate

✅ Can create, read, update, and delete memories via API
✅ Can inspect and clear L1 session summaries
✅ Dashboard handles 1,000+ memories without UX degradation
✅ Can trigger archival manually and see results immediately
✅ Can delete an agent and all associated data

---

## Phase 2 — Data Integrity

**Goal:** Make the memory store trustworthy at scale. Before this phase, the same fact can be stored 10 times; contradictions silently coexist; there's no way to bulk-manage data.

**Why now:** Every user who runs AEON-IQ for more than a day hits the dedup problem. Conflict detection is the #1 trust issue — if the system tells the LLM two contradictory things, the LLM's behavior becomes unpredictable. These must be solved before marketing the product.

### 2.1 — Dedup on insert `[~3 hrs]`

**Files:** `src/memory/store.rs`, `src/config.rs`

In `store_memory`, after embedding the fact but before INSERT:

1. Run a quick cosine search: `SELECT id, content FROM memories WHERE agent_id = $1 AND archived_at IS NULL ORDER BY embedding <=> $2 LIMIT 1`
2. If distance < `DEDUP_THRESHOLD` (new env var, default 0.05): skip the insert, log it, bump the existing memory's `access_count` and `last_accessed_at`.
3. Prometheus counter: `memoryos_dedup_skipped_total`

**Edge case:** If the near-duplicate has lower confidence or different provenance, consider updating the existing row's confidence upward (a repeated fact is more certain).

### 2.2 — Conflict detection `[~4 hrs]`

**Files:** new `src/memory/conflicts.rs`, `src/api.rs`, new migration

This is the hardest item in Phase 2. Two approaches:

**Option A — LLM-based (accurate, slower, costs money):**
On each insert, retrieve top-5 most similar memories. Send them + the new fact to the extractor LLM with a prompt: "Do any of these existing facts contradict the new fact? Return JSON with `conflicts: [{existing_id, reason}]` or `conflicts: []`."

**Option B — Heuristic (fast, free, less accurate):**
Pattern-match on entity + predicate. If an existing memory says "user is 25 years old" and the new one says "user is 30 years old", flag it based on entity overlap + numeric discrepancy.

**Recommendation:** Option A with the LLM call, but make it async (don't block the insert). Store conflicts in a new `memory_conflicts` table:

```sql
CREATE TABLE memory_conflicts (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id TEXT NOT NULL,
    memory_a UUID REFERENCES memories(id),
    memory_b UUID REFERENCES memories(id),
    reason TEXT NOT NULL,
    resolved_at TIMESTAMPTZ,
    resolution TEXT, -- 'keep_a', 'keep_b', 'keep_both', 'dismissed'
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
```

New API endpoints:
- `GET /api/v1/agents/:id/conflicts` — list unresolved conflicts
- `POST /api/v1/conflicts/:id/resolve` — resolve with chosen action

**Dashboard:** Add a "Conflicts" badge/tab showing unresolved conflict count.

### 2.3 — Bulk operations `[~2 hrs]` `[parallel]`

**File:** `src/api.rs`, `src/memory/store.rs`

Add `POST /api/v1/agents/:id/memories/bulk` with actions:

```json
{
  "action": "archive" | "delete",
  "filter": {
    "session_id": "optional",
    "memory_type": "optional",
    "older_than": "2025-01-01T00:00:00Z",
    "importance_below": 0.3
  }
}
```

Returns `{ "affected": 47 }`. Uses `QueryBuilder` for dynamic WHERE composition (pattern already established in `search_memories_filtered`).

### 2.4 — Export / Import `[~3 hrs]`

**File:** `src/api.rs`, `src/memory/store.rs`

**Export:** `GET /api/v1/agents/:id/export` → streams NDJSON. Each line is one memory with all fields except the raw embedding vector (too large, and must be re-embedded on import anyway).

**Import:** `POST /api/v1/agents/:id/import` → accepts NDJSON body. For each line: embed the content, store with original metadata (provenance, importance, memory_type). Run dedup check (2.1) on each. Return `{ "imported": 42, "skipped_dedup": 3, "errors": 0 }`.

This enables agent migration between instances and backup/restore.

### Phase 2 Gate

✅ Duplicate facts are caught and skipped on insert
✅ Contradictory facts are flagged (not auto-resolved)
✅ Operators can bulk-archive or bulk-delete by filter
✅ An agent's memory can be exported, wiped, and re-imported losslessly

---

## Phase 3 — Developer Experience & Distribution

**Goal:** Make AEON-IQ adoptable by developers who find it on GitHub. Before this phase, it's a codebase; after, it's a product.

**Why now:** Phases 0–2 made the system correct and complete. Phase 3 makes it discoverable, installable, and integratable without reading the source code.

### 3.1 — CI/CD pipeline `[~2 hrs]`

**File:** `.github/workflows/ci.yml`

```yaml
jobs:
  check:
    - cargo fmt --check
    - cargo clippy -- -D warnings
    - cargo test -- --skip memory::store::tests

  integration:
    services:
      postgres: pgvector/pgvector:pg16
    - cargo test  # full suite including sqlx::test

  dashboard:
    - cd dashboard && npm ci && npm run lint && npm run build

  docker:
    - docker compose build  # verify images build
```

Run on every push to `main` and every PR. No deployment yet — just verification.

### 3.2 — README overhaul `[~3 hrs]` `[parallel]`

Replace the current README (if minimal) with:

1. **Hero section** — one-sentence pitch, architecture diagram (Mermaid), badges (CI status, license, Docker pulls)
2. **30-second quickstart** — docker compose up, curl command, see memory in action
3. **How it works** — the 5-step request lifecycle as a visual flow
4. **Configuration reference** — table of all env vars (pull from config.rs, keep in sync)
5. **Integration examples** — Python/LangChain/CrewAI snippets (from CONTEXT_TRANSFER §6)
6. **Dashboard screenshots** — 2–3 actual screenshots
7. **Contributing** — link to CLAUDE.md for architecture, `cargo test` instructions
8. **License**

### 3.3 — Publish Docker images `[~1 hr]` `[parallel]`

**File:** `.github/workflows/publish.yml`

On tag push (`v*`), build and push to `ghcr.io/adaptive-liquidity/aeon-iq:latest` and `ghcr.io/adaptive-liquidity/aeon-iq:v0.1.0`. Same for the dashboard image.

Users can then:
```yaml
services:
  memoryos:
    image: ghcr.io/adaptive-liquidity/aeon-iq:latest
```

### 3.4 — MCP server wrapper `[~6 hrs]`

**New directory:** `mcp-server/`

Build a thin MCP server (TypeScript or Python, using the official MCP SDK) that wraps the management API. Tools exposed:

| Tool | Maps to |
|------|---------|
| `remember` | `POST /api/v1/agents/:id/memories` |
| `recall` | `POST /api/v1/memories/search` |
| `forget` | `DELETE /api/v1/memories/:id` |
| `list_memories` | `GET /api/v1/agents/:id/memories` |
| `get_sessions` | `GET /api/v1/agents/:id/sessions` |
| `get_conflicts` | `GET /api/v1/agents/:id/conflicts` |
| `export_agent` | `GET /api/v1/agents/:id/export` |

This lets Claude Desktop, Cursor, Windsurf, and any MCP-compatible tool use AEON-IQ's memory without HTTP plumbing.

**Config:** The MCP server reads `MEMORYOS_URL` and `MEMORYOS_API_KEY` from env. Ships as an npm package or a standalone binary.

### 3.5 — Python SDK `[~3 hrs]`

**New directory:** `sdks/python/` → publishes as `aeon-iq` on PyPI.

Thin typed wrapper:

```python
from aeon_iq import MemoryClient

client = MemoryClient(url="http://localhost:8080", api_key="...")
memories = client.search(agent_id="my-bot", query="user preferences", limit=5)
client.create(agent_id="my-bot", content="User prefers dark mode", importance=0.8)
client.export_agent("my-bot", path="backup.ndjson")
```

### 3.6 — TypeScript SDK `[~2 hrs]` `[parallel with 3.5]`

**New directory:** `sdks/typescript/` → publishes as `@aeon-iq/client` on npm.

Same surface as the Python SDK. Uses `fetch` with typed response interfaces.

### Phase 3 Gate

✅ CI passes on every PR (lint, test, build)
✅ A developer can go from GitHub README → running instance in < 5 minutes
✅ Docker images available on ghcr.io
✅ MCP server installable in Claude Desktop
✅ Python and TypeScript SDKs published

---

## Phase 4 — Intelligence Layer

**Goal:** Make retrieval smarter than cosine similarity. This is where AEON-IQ differentiates from "just pgvector with a cron job."

### 4.1 — Graph-powered retrieval `[~8 hrs]`

**Files:** `src/memory/retrieval.rs`, `src/memory/store.rs`

Currently the `entities` and `memory_graph` tables are populated during extraction but never used during retrieval.

**New retrieval path:**
1. Run the existing vector search (cosine + decay + importance)
2. Extract entity names from the query (simple NER: match against known entities for this agent)
3. If entities found: walk `memory_graph` — find all relations where the entity is subject or object
4. Fetch memories linked to those related entities (via content mention or a new `memory_entities` join table)
5. Merge and re-rank the combined set

**New migration:** `memory_entity_links` table that maps `memory_id → entity_id` (populated during extraction alongside the existing entity upsert).

**Config:** `GRAPH_RETRIEVAL_ENABLED` (default false). When enabled, retrieval does vector search + graph walk + merge.

### 4.2 — Entity disambiguation `[~4 hrs]`

**File:** `src/memory/store.rs`, `src/memory/extraction.rs`

Currently `upsert_entity` matches on exact `(agent_id, name)`. "Alex" and "Alexander" create two entities. "NovaPay" and "Nova Pay" are two entities.

**Fix:** Before upserting, check if any existing entity for this agent has a name within Levenshtein distance ≤ 2 or cosine similarity > 0.95 (embed the entity name). If so, merge into the existing entity.

### 4.3 — Structured working memory `[~4 hrs]`

**File:** `src/memory/extraction.rs`, `src/memory/store.rs`

Currently L1 is a single text `summary` field updated every turn. Replace with a structured JSON state:

```json
{
  "summary": "User is building a payment API...",
  "active_entities": ["NovaPay", "Stripe API"],
  "current_goal": "Debug the webhook handler",
  "open_questions": ["Which auth method to use?"],
  "turn_count": 12
}
```

The extraction prompt already outputs `updated_summary` — extend it to also output `active_entities`, `current_goal`, and `open_questions`. Store as JSONB in `working_memory.state` (new column alongside existing `summary` for backward compat).

### Phase 4 Gate

✅ Retrieval returns entity-related memories even when they're semantically distant
✅ "Alex" and "Alexander" resolve to the same entity
✅ Working memory carries structured state, not just a text blob
✅ Graph data is queryable via the dashboard

---

## Phase 5 — Scale & Monetize

**Goal:** Multi-tenant SaaS mode with enterprise features. This is the business-model phase.

### 5.1 — Multi-tenant isolation `[~8 hrs]`

**Approach:** Row-level security (RLS) in Postgres. Each API request's auth token resolves to a `tenant_id`. All tables get a `tenant_id` column. RLS policies ensure queries only see the current tenant's data.

**New migration:** Add `tenant_id TEXT` to all tables, create RLS policies, add `SET app.current_tenant` on every connection checkout.

### 5.2 — Billing meters `[~4 hrs]`

**File:** new `src/billing.rs`

Instrument counters per tenant: proxy requests, memories stored, embeddings generated, archival runs. Expose via `GET /api/v1/tenant/:id/usage`. Integrate with Stripe Meters or a custom billing table.

### 5.3 — Enterprise SSO `[~4 hrs]`

**File:** `dashboard/src/auth.ts`

Replace/augment the NextAuth Credentials provider with SAML (via `next-auth`'s enterprise providers) and OIDC (Google Workspace, Okta, Azure AD). Gate behind an `ENTERPRISE_SSO` feature flag.

### 5.4 — OpenTelemetry traces `[~4 hrs]`

**File:** `src/main.rs`, new `src/tracing_setup.rs`

Replace the current `tracing_subscriber` setup with an OpenTelemetry-compatible pipeline. Emit traces for: proxy request lifecycle, embedding calls, retrieval queries, extraction calls, archival runs. Export to Grafana Tempo or Honeycomb.

### 5.5 — GDPR compliance `[~3 hrs]`

**Endpoints:**
- `POST /api/v1/agents/:id/forget` — hard-delete all data for an agent (not tombstone — actual DELETE + VACUUM). Returns a compliance receipt.
- `GET /api/v1/agents/:id/audit-export` — full audit log export for compliance review.

### Phase 5 Gate

✅ Multiple tenants can share one instance without data leakage
✅ Per-tenant usage is metered and exportable
✅ Enterprise SSO works with at least one SAML provider
✅ Distributed traces flow from proxy to DB
✅ GDPR forget-me produces a verifiable deletion receipt

---

## Execution Summary

| Phase | Theme | Effort (est.) | Produces |
|-------|-------|---------------|----------|
| **0** | Stabilize | 1–2 days | A codebase you can build on safely |
| **1** | Complete MVP | 3–4 days | A product first users can actually use |
| **2** | Data Integrity | 4–5 days | A system you can trust at scale |
| **3** | Distribution | 5–7 days | A project developers can adopt from GitHub |
| **4** | Intelligence | 5–7 days | Differentiation beyond "just pgvector" |
| **5** | SaaS | 7–10 days | A business |

**Total estimated:** ~5–7 weeks of focused solo dev time with Claude Code.

---

## Prompt for Claude Code

To start executing, paste this into Claude Code at the beginning of each phase:

```
Read CONTEXT_TRANSFER.md and AEON-IQ_BUILD_BLUEPRINT.md in the project root.
I'm starting Phase [N]. Implement items [N.1] through [N.X] in order,
committing after each item with a conventional commit message.
Run `cargo check` and `cargo test -- --skip memory::store::tests` after each change.
Update CLAUDE.md if any architecture, endpoints, or env vars change.
```

---

*This blueprint is a living document. Update it as decisions change.*
