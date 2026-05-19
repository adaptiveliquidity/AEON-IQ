# AEON-IQ / MemoryOS Kernel — Context Transfer Document

> **Purpose:** Onboard a new Claude instance to this project with full design intent,
> conversation history, and decision rationale. The reader has already read the repo and
> CLAUDE.md; this document covers the *why*, not just the *what*.

---

## 1. Origin & Vision

### What problem does it solve?

Every AI agent forgets everything between sessions. The standard workarounds — stuffing the
full conversation history into the context window, or writing custom retrieval logic per
app — are either expensive, brittle, or both. MemoryOS solves this by sitting between the
caller and the LLM as a transparent proxy: it intercepts every chat-completions call,
retrieves relevant past facts from a vector database, and injects them as a system message.
On the way back it extracts new facts from the response in the background.

The critical UX property is **zero code changes**: point `OPENAI_BASE_URL` at the kernel
and add one header (`x-agent-id`). Existing OpenAI SDK code continues to work unmodified.

### Who is the target user?

Three concentric rings:

1. **Individual developers and hobbyists** — drop it in front of their agent scripts, get
   persistent memory for free in ten minutes (QUICKSTART covers this path).

2. **Agent framework builders** — teams building on LangChain, CrewAI, AutoGen, raw
   `openai` SDK, LiteLLM, or any OpenAI-compatible surface. The proxy pattern means no SDK
   integration work; the management API and dashboard give them observability.

3. **Enterprises running multi-agent pipelines** — need per-agent memory isolation,
   per-agent rate limiting, audit trails (tombstone-not-delete), importance-weighted
   retrieval, and a hosted/managed option. The multi-tenant SaaS path is on the roadmap
   but not yet built.

### Where does it fit in the Adaptive Liquidity Labs / AEON ecosystem?

The repository lives at `Adaptive-Liquidity/AEON-IQ`. "AEON" is the broader product
umbrella; "IQ" refers to the memory/intelligence layer. MemoryOS Kernel is the core
open-source infrastructure piece — the stateful, self-hostable component. The expectation
is that higher-level AEON products (trading bots, autonomous research agents, DeFi
execution systems) consume the kernel as their memory backend without coupling to any
specific LLM vendor.

Think of it as: **AEON is the agent platform; MemoryOS Kernel is its hippocampus.**

### 6-month product vision (as understood from the build so far)

The trajectory being built toward:

- **Month 1–2** (current): Open-source MVP with full L1/L2/L3 memory, decay, importance,
  versioned archival, dashboard, and multi-provider support. Publish and get adoption.
- **Month 3**: MCP server wrapper so Claude Desktop and any MCP-compatible agent can use
  this without touching an API. SDK wrappers (Python, TypeScript) that provide a typed
  memory client instead of raw HTTP.
- **Month 4**: Conflict detection and resolution (two memories that contradict each other),
  memory deduplication, graph-powered retrieval (walk entity/relation graph to answer
  structural questions the vector search can't), working memory management API.
- **Month 5**: Multi-tenant SaaS mode with per-tenant Postgres schemas or row-level
  security, billing integration, hosted offering on managed infra.
- **Month 6**: OpenTelemetry traces, enterprise SSO (SAML/OIDC), compliance export (GDPR
  forget-me, SOC2 audit logs), and production hardening.

---

## 2. Design Decisions & Trade-offs

### Why Rust over Go or Python for the kernel?

**Decision: Rust.**

Rationale:
- The kernel is on the hot path of every LLM call. Latency added by the kernel must be
  negligible (target: < 5 ms for memory retrieval + injection, excluding upstream). Rust
  gives predictable, zero-GC latency with no JVM/GC pauses.
- A single statically-linked binary + Docker layer. No Python environment management,
  no interpreter version drift.
- Axum + Tokio give async streaming with back-pressure out of the box. Streaming a 20K
  token response through the proxy while simultaneously writing to Postgres in the
  background is idiomatic in async Rust; equivalent Go or Python code is messier.
- `sqlx` gives compile-time-checked SQL (we deliberately chose the non-macro form
  `sqlx::query` to avoid the compile-time DB dependency, but could opt in later).
- Safety: the kernel holds API keys and user memory; Rust's ownership model eliminates
  entire classes of memory safety bugs that are live concerns in C/Go proxies.

The main cost is compile time (the Docker build takes ~60 s cold; incremental < 5 s) and
a steeper onboarding curve for contributors who don't know Rust.

### Why the proxy pattern instead of an SDK or library?

**Decision: OpenAI-compatible HTTP proxy.**

The alternative was a Python/TypeScript library that developers import and call
explicitly (e.g. `memory.remember(...)`, `memory.recall(...)`). We rejected this because:

- **Integration surface**: every agent framework, every language, every LLM client would
  need a separate wrapper. The proxy requires exactly one env var change.
- **Transparency**: the agent doesn't know memory is happening. This is a feature, not a
  bug — it means memory can be added to a system that's already in production without any
  refactoring.
- **Future-proofing**: if OpenAI changes their SDK, the proxy still works. We own the
  interface.

The proxy pattern is explicitly documented in QUICKSTART as the core value proposition.

### Why pgvector (Postgres) over Pinecone / Qdrant / Weaviate?

**Decision: pgvector inside the existing Postgres instance.**

The alternatives were purpose-built vector databases. We chose pgvector because:

- **Operational simplicity**: one database, one connection pool, one backup strategy. You
  don't add a second stateful system (Qdrant, Pinecone) to a self-hosted deployment.
- **Transactional safety**: tombstoning an archived memory and creating L3 facts happen in
  the same Postgres transaction semantics, not across two systems.
- **Rich queries**: the two-CTE retrieval pattern (cosine distance + decay + importance
  weighting) would be impossible or awkward in a pure vector DB. In Postgres it's just SQL.
- **pgvector HNSW** (added in 0.5.0, used here) gives sub-millisecond ANN search at the
  scale we care about (< 100K memories per agent in the MVP).
- **Familiarity**: every devops person knows how to run Postgres. Nobody wants to learn
  Qdrant internals at 2 AM when the index is corrupted.

The trade-off is that pgvector won't match Pinecone at billions of vectors. That's a
problem for a future version; for the current target users (agents with thousands to
hundreds of thousands of memories per agent) it's irrelevant.

### Why buffer-and-resynthesize for Anthropic instead of true streaming?

**Decision: buffer the entire Anthropic response server-side, then re-emit as synthetic
OpenAI SSE.**

The alternative was to translate Anthropic's SSE format (`event: content_block_delta`,
`data: {"type":"content_block_delta",...}`) to OpenAI's SSE format in real time, chunk
by chunk.

We chose buffering because:

- **Wire-format complexity**: Anthropic's streaming protocol has state (`message_start`,
  `content_block_start`, multiple delta types, `message_delta`, `message_stop`). Mapping
  this to OpenAI's simpler `data: {"choices":[{"delta":{"content":"..."}}]}` in a
  streaming transformer requires maintaining non-trivial parser state.
- **Memory extraction requires the full response**: the background extraction job needs
  the complete assistant turn to extract facts. If we stream, we have to buffer anyway;
  might as well do it once server-side.
- **Agent use cases tolerate the latency**: LLM proxy calls in agent pipelines are almost
  always batch-style (one shot, wait for full response). True token-by-token streaming
  matters for chat UIs; agent orchestration loops rarely care.

The trade-off (documented in `proxy.rs` as a comment) is that first-byte latency for
Anthropic equals the full upstream response time, not time-to-first-token. This is
acceptable for the current target use case.

Gemini uses its own `/v1beta/openai/` compatible surface, so it takes the OpenAI
streaming path with no translation needed.

### Why the importance score system instead of pure cosine similarity?

**Decision: three-factor retrieval with configurable decay and importance weighting.**

Pure cosine similarity retrieves the most *semantically similar* memory, but not
necessarily the most *useful* one. A trivial memory stored yesterday can outrank a
critical business decision stored a year ago if the embeddings happen to be similar.

We added:
- **Decay rate** (`MEMORY_DECAY_RATE`): stale memories with no recent retrieval hits
  get penalized proportionally to `days_since_last_access`. Defaults to 0.0 (disabled)
  so existing deployments see no behavior change.
- **Importance weighting** (`IMPORTANCE_BOOST_FACTOR`): high-importance memories are
  pulled closer in the ranked list. Importance is assigned by three signals in priority
  order: caller override header (`x-memory-importance`), agent XML tags (`<important>`),
  or LLM extractor scoring. Defaults to 0.0 (disabled).
- **Refresh-on-read** (`IMPORTANCE_REFRESH_BOOST`): every retrieval bumps `importance_score`
  by a small amount (default 0.05), implementing a spacing-effect memory reinforcement.

With all boosts at 0.0, the formula collapses to pure cosine similarity — fully backward
compatible. Operators opt in by setting the env vars.

### Why non-destructive tombstoning instead of DELETE?

**Decision: `archived_at = NOW()` instead of `DELETE FROM memories`.**

All queries filter `AND archived_at IS NULL`. Tombstoned memories are invisible to
retrieval but permanently retained in the database. Motivation:

- **Audit trail**: for compliance use cases, you need to prove what the agent "knew"
  at a given point in time.
- **Reversibility**: the entire reversible archival feature depends on being able to
  un-tombstone L2 memories after an L3 compaction. Hard-delete would make this
  impossible without a separate archive table.
- **Lineage**: `archival_batch_id` links tombstoned L2 sources to their L3 compressed
  replacements, enabling full compaction history with a single foreign key.

---

## 3. Current State — What Works

### Fully implemented and integration-tested

| Feature | How tested |
|---------|-----------|
| OpenAI-compatible proxy (streaming + non-streaming) | Unit tests in `providers.rs`; smoke test via `test_memory.py` |
| Anthropic provider (buffer + resynthesize) | Unit tests in `providers.rs` |
| Memory extraction (MMU v2 with provenance) | Unit tests in `extraction.rs` |
| Batch embedding (one API call per turn) | Code path in `embeddings.rs` |
| L1/L2/L3 memory tiers | DB schema + retrieval logic |
| Three-factor retrieval (cosine + decay + importance) | `#[sqlx::test]` integration tests |
| Importance scoring (3 signals + refresh-on-read) | `#[sqlx::test]` integration test |
| LTM archival job (L2→L3 compaction) | Code complete; not integration-tested (needs live LLM) |
| Versioned archival batches | `#[sqlx::test]` integration tests (full round-trip + idempotency) |
| Reversible archival (individual + batch restore) | `#[sqlx::test]` integration tests |
| Per-agent rate limiting (token bucket) | Unit tests in `rate_limit.rs` |
| Management API (CRUD, search, archival) | Code complete; smoke-testable via curl |
| Next.js dashboard | Builds clean; not E2E tested |
| Prometheus metrics + Grafana | Provisioned via docker-compose |
| 20-turn retention test | `#[sqlx::test]` integration test |

### Partially scaffolded

- **Knowledge graph** (`entities`, `memory_graph` tables, `get_agent_relations` store
  function): the tables exist and entity/relation extraction is wired into the MMU
  pipeline. The dashboard "Knowledge Graph" tab surfaces them. However, the graph is
  *display-only* — there's no graph-walk retrieval, no entity disambiguation, and no
  conflict detection.

- **Sessions table** (`0001_initial.sql`): the table exists but nothing currently reads
  from it. `session_id` is tracked on memories and working_memory but the sessions table
  itself is orphaned scaffolding.

- **Gemini provider**: uses Gemini's OpenAI-compatibility layer (`/v1beta/openai/`). Works
  for basic calls; not tested against real Gemini endpoints.

### Known-not-working or untested

- **End-to-end extraction** requires a real API key. The `test_memory.py` smoke test covers
  the golden path but CI runs without keys.
- **Dashboard E2E**: Next.js builds clean and the component logic is correct but there are
  no Playwright/Cypress tests. Visual regressions are caught manually.
- **Archival job with real LLM**: the compaction prompt and parsing are coded but have never
  been run against a live model in this codebase.

---

## 4. Known Issues & Tech Debt

### Intentional MVP shortcuts

- **No memory deduplication**: if the same fact is extracted across multiple turns (e.g.
  "user's name is Alex" appears in turns 3 and 7), both copies are stored. The retrieval
  ranking naturally deprioritizes duplicates over time via decay, but it's wasteful. A
  similarity-threshold dedup check on insert is the right fix.

- **No PATCH endpoint for memories**: you can create or delete a memory but you can't
  correct its content. This is table-stakes for any serious data management UI. The store
  function would be trivial; the API route and dashboard UI are the work.

- **No working memory management API**: L1 session summaries exist in the `working_memory`
  table and are read during retrieval, but there's no endpoint to list or clear them. An
  operator can't inspect what the current session summary looks like without hitting the DB
  directly.

- **Dashboard pagination**: the browse tab loads up to 50 memories (`limit` hardcoded in
  `list_memories`). There's no "load more" or page controls. For agents with thousands of
  memories this is a UX problem.

- **No bulk operations**: no filter-based bulk archive/delete (e.g. "archive all episodic
  memories from session X older than 30 days"). Every action is per-memory or per-batch.

- **No manual archival trigger**: the compaction job only runs on the cron schedule
  (`ARCHIVAL_INTERVAL_HOURS`). There's no `POST /api/v1/agents/:id/archival/trigger`
  endpoint to fire it on demand.

### Non-blocking `unwrap()` calls in production paths

Several `.unwrap()` calls exist in non-test code, primarily in `proxy.rs` and
`providers.rs`. These are on paths where the failure modes are either:
- A response builder that can't fail given valid inputs (`builder.body(...)`)
- JSON serialization of values we just deserialized (`serde_json::to_value(req)`)

None are on DB or network I/O. They're acceptable for MVP but should be converted to
proper error propagation before the hosted offering.

### Connection pool sizing

`db.rs` uses `PgPool::connect()` with all defaults. For production you'd want explicit
`max_connections`, `acquire_timeout`, and `idle_timeout` settings. The `docker-compose.yml`
Postgres container also has no tuning applied.

### Archival job error handling

If the LLM compaction call fails mid-run (e.g. rate-limited), the batch record is created
but left in `status = 'completed'` with 0 L3 facts. The L2 memories are not tombstoned
(we bail early on embedding failure), so no data is lost — but the orphan batch record
is confusing. A `status = 'failed'` terminal state would be cleaner.

### HNSW index not rebuilt after bulk deletes

The HNSW index (`idx_memories_hnsw`) is not periodically rebalanced. Tombstoning many
memories doesn't trigger a VACUUM + REINDEX. This is fine at MVP scale but will degrade
query performance on long-running deployments with heavy archival. Periodic `REINDEX
CONCURRENTLY` should be added to any production runbook.

---

## 5. Roadmap — Next Steps

### Immediate (next 1–2 features, high confidence)

| Item | Why now |
|------|---------|
| **A. Memory editing (PATCH)** | Table-stakes for the dashboard to be a real management UI. Trivial backend (`UPDATE memories SET content = $1 WHERE id = $2`); requires re-embedding the new content. |
| **B. Working memory API** | Operators need to inspect L1 session summaries. `GET /api/v1/agents/:id/sessions` + `DELETE /api/v1/agents/:id/sessions/:session_id`. |
| **C. Dashboard pagination** | Browse tab hard-stops at 50 memories; needs offset/limit controls or infinite scroll. |

### Medium-term (next sprint, high value)

| Item | Notes |
|------|-------|
| **D. Memory deduplication** | On-insert cosine similarity check: if a memory with distance < 0.05 already exists for this agent, skip storage or merge. Configurable threshold. |
| **E. Conflict detection** | Two memories with contradictory content (e.g. "user is 25 years old" vs "user is 30 years old"). Requires either a second LLM call or a heuristic. Flag conflicts; don't auto-resolve without user intent. |
| **F. Manual archival trigger** | `POST /api/v1/agents/:id/archival/trigger` — fires one compaction cycle synchronously or returns a job ID. |
| **G. Export / import** | `GET /api/v1/agents/:id/export` → NDJSON. `POST /api/v1/agents/:id/import` → bulk insert with re-embedding. Enables agent migration and backup. |

### Longer-term (3–6 months)

| Item | Notes |
|------|-------|
| **MCP server wrapper** | Expose MemoryOS as an MCP server so Claude Desktop and any MCP-compatible framework can query and store memories without an HTTP client. |
| **Python + TypeScript SDKs** | Thin typed wrappers around the management API. Not a replacement for the proxy pattern — a complement for operators who want a programmatic memory client. |
| **Graph-powered retrieval** | Walk the entity/relation graph during retrieval: if the query mentions "NovaPay" (an entity), also retrieve memories linked to NovaPay's relations even if they're semantically distant from the query vector. |
| **Multi-tenant SaaS mode** | Row-level security per tenant, per-tenant billing meters on the management API, hosted offering on managed Postgres (Neon/Supabase). |
| **OpenTelemetry traces** | Distributed tracing from proxy entry through embedding, retrieval, storage, and archival. Connects to Grafana Tempo or Honeycomb. |
| **Enterprise SSO** | SAML / OIDC for the dashboard. Currently uses NextAuth credentials (email + password). |

---

## 6. Integration Patterns

### Pattern 1: Drop-in OpenAI SDK (Python)

The zero-code-change path. Works with any OpenAI-compatible client.

```python
from openai import OpenAI

client = OpenAI(
    api_key="sk-...",                          # real OpenAI key — still passed through
    base_url="http://localhost:8080/v1",       # point at the kernel
    default_headers={"x-agent-id": "my-bot"}  # one new header
)
# From here, use exactly the OpenAI API you already know.
```

### Pattern 2: LangChain

```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(
    model="gpt-4o-mini",
    base_url="http://localhost:8080/v1",
    default_headers={"x-agent-id": "langchain-agent"},
)
```

### Pattern 3: CrewAI / AutoGen

Both frameworks use the OpenAI SDK under the hood. Set `OPENAI_BASE_URL=http://localhost:8080/v1`
as an environment variable and add `x-agent-id` to the default headers on the underlying
client. No framework-specific integration needed.

### Pattern 4: Anthropic native (via kernel)

Point the kernel at `https://api.anthropic.com` with `UPSTREAM_PROVIDER=anthropic` and
continue using the OpenAI SDK — the kernel translates the request/response format. The
caller never changes.

```bash
UPSTREAM_PROVIDER=anthropic
UPSTREAM_BASE_URL=https://api.anthropic.com
OPENAI_API_KEY=sk-ant-...   # Anthropic key, passed through as Bearer
```

### Pattern 5: Local models (Ollama / LM Studio)

```bash
UPSTREAM_BASE_URL=http://host.docker.internal:11434
UPSTREAM_PROVIDER=openai
# Use your local model's embedding endpoint or set EMBEDDING_BASE_URL separately
EMBEDDING_BASE_URL=https://api.openai.com  # keep using OpenAI for embeddings
```

### Pattern 6: Per-user isolation in a multi-user app

Each user gets their own `agent_id`. Sessions within a user are tracked by `x-session-id`
(auto-generated UUID if not supplied). The dashboard scopes non-admin users to their own
`agentId` (derived from their login email: `email.toLowerCase().replace(/[@.+]/g, "-")`).

### Pattern 7: Importance signalling

For facts the calling agent knows are critical (user identity, compliance rules):

```python
client.chat.completions.create(
    model="gpt-4o-mini",
    messages=[...],
    extra_headers={"x-memory-importance": "0.95"}
)
```

Or within an assistant response, wrap content in `<important>...</important>` tags to
floor the importance score at 0.9 for those facts.

---

## 7. Naming & Branding Context

There are three overlapping names and it's intentional:

| Name | Usage |
|------|-------|
| **AEON-IQ** | GitHub repository name (`Adaptive-Liquidity/AEON-IQ`). The product-level name in the AEON ecosystem. Use when referring to the project as a whole. |
| **MemoryOS** | The conceptual name for the memory operating system. Used in the dashboard title, the docker-compose service name (`memoryos`), Prometheus metric prefix (`memoryos_*`), and the log filter (`RUST_LOG=memoryos_kernel=info`). Use when explaining what the system does. |
| **MemoryOS Kernel** | The Rust binary specifically (`Cargo.toml` package: `memoryos-kernel`, binary: `memoryos`). Use when referring to the self-hosted, open-source core component as distinct from future hosted/cloud layers. |

The intended mental model: **AEON-IQ is the product. MemoryOS is the concept. The kernel
is the open-source implementation.**

In documentation aimed at developers (QUICKSTART, CLAUDE.md), "MemoryOS Kernel" is used.
In product/marketing contexts, "AEON-IQ" is used. Both refer to the same running binary.

---

## Appendix: Key File Map

| File | Purpose |
|------|---------|
| `src/proxy.rs` | Request lifecycle: auth → rate limit → retrieve → forward → tee → background extract |
| `src/providers.rs` | OpenAI / Anthropic / Gemini wire-format translation |
| `src/memory/store.rs` | All DB operations: store, search, tombstone, archival batches, restore |
| `src/memory/extraction.rs` | MMU v2: LLM extraction prompt, provenance-adjusted confidence, importance signal resolution |
| `src/memory/retrieval.rs` | Assembles retrieved memories + L1 summary into injected context |
| `src/archival.rs` | Background LTM compaction job: L2→L3 compression + batch versioning |
| `src/api.rs` | Management REST handlers (CRUD, search, archival batch ops) |
| `src/config.rs` | All env vars with defaults; single source of truth |
| `src/metrics.rs` | Prometheus counter/histogram definitions |
| `src/rate_limit.rs` | DashMap-backed per-agent token bucket |
| `migrations/` | Additive SQL migrations (0001–0006); run automatically on startup |
| `dashboard/` | Next.js 15 app with NextAuth v5, memory explorer, Prometheus-linked stats |

---

*Generated 2026-05-19. Branch: `claude/memoryos-kernel-mvp-1B8Rg`.*
