# AGENTS.md

## Cursor Cloud specific instructions

AEON-IQ / MemoryOS is a Rust kernel proxy + Next.js dashboard monorepo. See `CLAUDE.md` and `README.md` for architecture and API details.

### Docker in Cloud Agent VMs

Docker is required for the recommended dev stack (Postgres + pgvector, mock OpenAI, kernel, dashboard). On first use in a fresh VM:

1. Ensure Docker daemon config uses `fuse-overlayfs` (`/etc/docker/daemon.json`).
2. Start the daemon if not running: `sudo dockerd --host=unix:///var/run/docker.sock` (background).
3. Use **test compose** for offline E2E (no real OpenAI key):  
   `sudo docker compose -f docker-compose.test.yml up --build -d`

Services: kernel `:8080`, dashboard `:3000`, Postgres `:5432`, mock OpenAI `:11435`.

Management API key in test compose: `test-management-key` (header `X-Management-Key` or `Authorization: Bearer test-management-key`). `ALLOW_UNAUTH_MANAGEMENT` is `false` in test compose — management routes require the key.

### Rust toolchain

The kernel requires **Rust 1.96+** (`edition2024`). Run `rustup default stable` if `cargo check` fails on edition. Local builds also need system packages `pkg-config` and `libssl-dev` (not in the update script).

### Running without Docker (kernel only)

1. `docker compose -f docker-compose.test.yml up -d postgres` (or full stack)
2. Export `DATABASE_URL=postgresql://memoryos:memoryos_secret@localhost:5432/memoryos` plus other vars from `.env.example`
3. `cargo run` from repo root

### Lint / test / build (standard commands)

| Component | Command | Notes |
|-----------|---------|-------|
| Rust fmt | `cargo fmt --check` | |
| Rust lint | `cargo clippy -- -D warnings` | |
| Rust unit tests | `cargo test -- --skip memory::store::tests` | No DB |
| Rust integration tests | `DATABASE_URL=postgresql://memoryos:memoryos_secret@localhost:5432/memoryos cargo test` | Needs Postgres |
| Dashboard lint | `cd dashboard && npm run lint` | |
| Dashboard build | `cd dashboard && npm run build` | Set `AUTH_SECRET`, `NEXTAUTH_URL`, etc. (see CI workflow) |
| MCP server | `cd mcp-server && npm ci && npm run build` | |

### Hello-world / smoke test

With test stack running:

```bash
python3 run_tests.py   # needs X-Management-Key support in script; many mgmt checks 401 without it
```

Reliable offline demo (mock upstream triggers):

1. `POST /v1/chat/completions` with `x-agent-id`, session 1: *"My name is Alex. I work at NovaPay."*
2. Wait ~8s for background extraction
3. Session 2 user message must match mock patterns in `mock_openai_server.py`, e.g. *"what is my name and what startup am I building?"*
4. Verify via `GET /api/v1/agents/{id}/memories` with management key, or recall in chat response

### Dashboard auth caveat

Test compose sets `MANAGEMENT_API_KEY` and NextAuth env vars. If login or Memory Explorer API calls fail with Unauthorized, confirm the dashboard container has `MANAGEMENT_API_KEY=test-management-key` and `AUTH_SECRET` set (see `docker-compose.test.yml`). The kernel proxy path (`/v1/chat/completions`) does not require management auth.
