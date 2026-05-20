# AEON-IQ / MemoryOS Kernel

> **Give any AI agent persistent memory with zero code changes.**

[![CI](https://github.com/adaptive-liquidity/aeon-iq/actions/workflows/ci.yml/badge.svg)](https://github.com/adaptive-liquidity/aeon-iq/actions/workflows/ci.yml)
[![License: MIT](https://img.shields.io/badge/License-MIT-blue.svg)](LICENSE)
[![Docker](https://img.shields.io/badge/docker-ghcr.io%2Fadaptive--liquidity%2Faeon--iq-blue)](https://ghcr.io/adaptive-liquidity/aeon-iq)

MemoryOS Kernel is a **transparent OpenAI-compatible proxy** that sits between your agent and any LLM. It automatically retrieves relevant past facts on every request and extracts new facts from every response — all in the background, with no changes to your existing code.

```
Your App  ──→  POST /v1/chat/completions  ──→  MemoryOS Kernel  ──→  OpenAI / Anthropic / Ollama
                                                      │                        │
                                               inject memories          stream response
                                               (< 5 ms overhead)              │
                                                      └──── [background] extract + store facts
```

## 30-second quickstart

```bash
git clone https://github.com/adaptive-liquidity/aeon-iq
cd aeon-iq
cp .env.example .env          # add your OPENAI_API_KEY
docker compose up --build     # ~60 s first build, instant after
```

Then point any OpenAI SDK client at the kernel:

```python
from openai import OpenAI

client = OpenAI(
    api_key="sk-...",                          # your real key — passed through unchanged
    base_url="http://localhost:8080/v1",       # ← one env var change
    default_headers={"x-agent-id": "my-bot"}  # ← one new header
)

# Everything else is exactly the same OpenAI API you already know.
response = client.chat.completions.create(
    model="gpt-4o-mini",
    messages=[{"role": "user", "content": "My name is Alex. I work at NovaPay."}]
)
# Later, in a new session...
response = client.chat.completions.create(
    model="gpt-4o-mini",
    messages=[{"role": "user", "content": "What's my name?"}]
)
# → "Your name is Alex, and you work at NovaPay."
```

| Service   | URL                    |
|-----------|------------------------|
| Kernel    | http://localhost:8080  |
| Dashboard | http://localhost:3000  |
| Postgres  | localhost:5432         |

---

## How it works

Every `POST /v1/chat/completions` passes through five stages:

```
┌────────────────────────────────────────────────────────────────┐
│                       MemoryOS Kernel                          │
│                                                                │
│  [1] Auth & rate-limit ── extract x-agent-id / x-session-id   │
│             │                                                  │
│             ▼                                                  │
│  [2] Retrieve ── embed user message → pgvector similarity      │
│        search → inject top memories as <retrieved_memories>    │
│        system message                                          │
│             │                                                  │
│             ▼                                                  │
│  [3] Translate & forward ── adapt to upstream provider format  │
│             │                                                  │
│             ▼                                                  │
│  [4] Tee response ── stream chunks to client while capturing   │
│             │                                                  │
│             ▼ (background, non-blocking)                       │
│  [5] Extract & store ── LLM extracts structured facts,         │
│        batch-embeds them, stores to Postgres + updates L1      │
│        session summary                                         │
└────────────────────────────────────────────────────────────────┘
```

### Memory tiers

| Tier | Storage | Description |
|------|---------|-------------|
| **L1** | `working_memory` | Per-session rolling summary; updated every turn |
| **L2** | `memories` (tier='L2') | Individual extracted facts; default tier |
| **L3** | `memories` (tier='L3') | Compressed archival facts; created by the LTM job |

The **LTM archival job** automatically compacts stale L2 facts into concise L3 summaries on a configurable schedule. Every compaction run is versioned and reversible — you can restore any batch with a single API call.

### Retrieval formula

```
adjusted_dist = cosine_dist
    × (1 + MEMORY_DECAY_RATE × days_stale)
    × (1 + IMPORTANCE_BOOST_FACTOR × (1 − importance_score))
```

When both factors are 0 (default), this collapses to pure cosine similarity.

### Importance scoring

Memories get an importance score from the highest-priority available signal:

1. **Caller header** — `x-memory-importance: 0.95` (operator override)
2. **Agent XML tags** — `<important>…</important>` in assistant responses (floored at 0.9)
3. **LLM extractor** — four-tier rubric: 1.0 = critical, 0.8–0.99 = high, 0.5–0.79 = standard, 0–0.49 = trivial

---

## Integration examples

### Drop-in OpenAI SDK (Python)

```python
from openai import OpenAI

client = OpenAI(
    api_key="sk-...",
    base_url="http://localhost:8080/v1",
    default_headers={"x-agent-id": "my-bot"}
)
```

### LangChain

```python
from langchain_openai import ChatOpenAI

llm = ChatOpenAI(
    model="gpt-4o-mini",
    base_url="http://localhost:8080/v1",
    default_headers={"x-agent-id": "langchain-agent"},
)
```

### CrewAI / AutoGen

Both frameworks use the OpenAI SDK under the hood. Set one environment variable:

```bash
export OPENAI_BASE_URL=http://localhost:8080/v1
# Add x-agent-id to the default_headers on the underlying OpenAI client
```

### Anthropic (via kernel translation)

Use the OpenAI SDK — the kernel translates the wire format automatically:

```bash
UPSTREAM_PROVIDER=anthropic
UPSTREAM_BASE_URL=https://api.anthropic.com
OPENAI_API_KEY=sk-ant-...   # your Anthropic key
```

### Local models (Ollama / LM Studio)

```bash
UPSTREAM_BASE_URL=http://host.docker.internal:11434
UPSTREAM_PROVIDER=openai
EMBEDDING_BASE_URL=https://api.openai.com   # keep OpenAI for embeddings
```

### Signal important facts

```python
# Override importance for a turn (0.0–1.0):
client.chat.completions.create(
    model="gpt-4o-mini",
    messages=[...],
    extra_headers={"x-memory-importance": "0.95"}
)
```

Or wrap critical content in assistant responses:

```
<important>User's compliance requirement: must use AES-256.</important>
```

---

## Request headers

| Header | Required | Description |
|--------|----------|-------------|
| `x-agent-id` | **Yes** | Unique identifier for the agent / user |
| `x-session-id` | No | Groups turns into one session (UUID auto-generated if absent) |
| `x-memory-importance` | No | Override importance score 0.0–1.0 for this turn's memories |

---

## Configuration reference

| Variable | Default | Description |
|----------|---------|-------------|
| `DATABASE_URL` | *(required)* | PostgreSQL connection string |
| `OPENAI_API_KEY` | — | API key forwarded to upstream LLM; used for embeddings and extraction |
| `UPSTREAM_PROVIDER` | `openai` | Provider: `openai` \| `anthropic` \| `gemini` |
| `UPSTREAM_BASE_URL` | `https://api.openai.com` | LLM provider base URL |
| `EMBEDDING_BASE_URL` | `UPSTREAM_BASE_URL` | Override for embedding calls only |
| `EXTRACTOR_BASE_URL` | `https://api.openai.com` | Override for fact-extraction LLM |
| `EXTRACTOR_MODEL` | `gpt-4o-mini` | Model used for background fact extraction |
| `EMBEDDING_MODEL` | `text-embedding-3-small` | Embedding model |
| `EMBEDDING_DIMENSION` | `1536` | Must match `vector(N)` in schema |
| `RETRIEVAL_THRESHOLD` | `0.80` | Cosine distance upper bound (lower = stricter) |
| `MEMORY_DECAY_RATE` | `0.0` | Per-day staleness penalty; 0 = disabled |
| `IMPORTANCE_BOOST_FACTOR` | `0.0` | Importance weight in retrieval; 0 = disabled |
| `IMPORTANCE_REFRESH_BOOST` | `0.05` | Per-retrieval importance bump; 0 = disabled |
| `RATE_LIMIT_RPM` | `0` | Per-agent requests per minute cap; 0 = disabled |
| `RATE_LIMIT_BURST` | `20` | Token bucket burst size |
| `MANAGEMENT_API_KEY` | *(unset)* | Protects `/api/v1/*`; unauthenticated if unset |
| `ARCHIVAL_INTERVAL_HOURS` | `24` | LTM compaction job frequency; 0 = disabled |
| `ARCHIVAL_MIN_AGE_DAYS` | `7` | Minimum age before an L2 memory is a compaction candidate |
| `ARCHIVAL_MIN_MEMORIES` | `10` | Minimum candidate count before triggering compaction |
| `PORT` | `8080` | Kernel listen port |

---

## Management API

Secure with `X-Management-Key` or `Authorization: Bearer` headers (set `MANAGEMENT_API_KEY`).

| Method | Path | Description |
|--------|------|-------------|
| GET | `/api/v1/agents` | List all agents |
| GET | `/api/v1/agents/:id/memories` | Paginated live memories |
| POST | `/api/v1/agents/:id/memories` | Create memory manually |
| GET | `/api/v1/agents/:id/memories/archived` | Tombstoned memories |
| GET | `/api/v1/agents/:id/archival/batches` | Archival batch history |
| POST | `/api/v1/memories/search` | Semantic search |
| DELETE | `/api/v1/memories/:id` | Hard-delete a memory |
| POST | `/api/v1/memories/:id/restore` | Restore a tombstoned memory |
| POST | `/api/v1/archival/batches/:id/restore` | Restore an entire archival batch |
| GET | `/api/v1/stats` | Agent + memory counts |

### Example: inspect memories

```bash
curl -H "X-Management-Key: $MANAGEMENT_API_KEY" \
  http://localhost:8080/api/v1/agents/my-bot/memories
```

### Example: semantic search

```bash
curl -X POST \
  -H "X-Management-Key: $MANAGEMENT_API_KEY" \
  -H "Content-Type: application/json" \
  -d '{"query": "user preferences", "agent_id": "my-bot", "limit": 5}' \
  http://localhost:8080/api/v1/memories/search
```

---

## Dashboard

Open **http://localhost:3000** after `docker compose up`.

- **Overview** — agent count, total memories, injection hit rate, estimated token savings
- **Memory Explorer** — browse, search, manually add, and delete memories per agent
  - **Archived** tab — view tombstoned memories; restore individuals with one click
  - **Batch History** — every L2→L3 compaction run with full restore support
- **Knowledge Graph** — entity/relation graph extracted from conversations
- **Metrics** — Prometheus-backed usage charts (requests, extraction rate, retrieval hits)

Default credentials: `admin@memoryos.dev` / `changeme` (set `DASHBOARD_ADMIN_EMAIL` and `DASHBOARD_ADMIN_PASSWORD`).

---

## Observability

Prometheus metrics at `GET /metrics` (text format 0.0.4):

| Metric | Type | Description |
|--------|------|-------------|
| `memoryos_requests_total` | Counter | Total proxy requests |
| `memoryos_extraction_total{status}` | Counter | Extraction outcomes: ok \| error \| low_confidence |
| `memoryos_injection_total{result}` | Counter | Retrieval: hit \| miss |
| `memoryos_rate_limited_total` | Counter | Requests blocked by rate limiter |

Grafana dashboard provisioned automatically at **http://localhost:3001**.

---

## Production notes

1. Replace `memoryos_secret` with a strong Postgres password
2. Set `MANAGEMENT_API_KEY` to a secret value
3. Put Nginx/Caddy in front of port 8080 (TLS termination)
4. Set `RUST_LOG=memoryos_kernel=warn` to reduce log volume
5. Add Postgres replicas/backups for the `postgres_data` volume
6. The kernel is stateless — scale horizontally behind a load balancer

---

## Contributing

See [CLAUDE.md](CLAUDE.md) for architecture overview and design decisions.

```bash
# Fast type-check (no DB needed)
cargo check

# Unit tests (no DB needed)
cargo test -- --skip memory::store::tests

# Full tests (requires pgvector Postgres)
docker compose up -d postgres
cargo test

# Dashboard
cd dashboard && npm install && npm run dev
```

Rust edition 2021, sqlx 0.8 (non-macro form — no compile-time DB dependency). All PRs run the full CI matrix before merge.

---

## License

MIT — see [LICENSE](LICENSE).
