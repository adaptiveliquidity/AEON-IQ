# aeon-iq Python SDK

Typed Python client for [AEON-IQ MemoryOS](../../README.md) — persistent memory for AI agents.

## Installation

```bash
pip install aeon-iq
```

## Quick start

```python
from aeon_iq import MemoryClient

client = MemoryClient(url="http://localhost:8080", api_key="...")

# Store a fact
client.create(agent_id="my-bot", content="User prefers dark mode", importance=0.8)

# Semantic search
results = client.search(agent_id="my-bot", query="user preferences", limit=5)
for m in results.memories:
    print(m.content, m.importance_score)

# Export to NDJSON
client.export_agent("my-bot", path="backup.ndjson")

# Import from NDJSON
result = client.import_agent("my-bot", path="backup.ndjson")
print(f"Imported {result.imported} memories")
```

## Configuration

The client reads `MEMORYOS_URL` and `MEMORYOS_API_KEY` from the environment if not passed directly:

```bash
export MEMORYOS_URL=http://localhost:8080
export MEMORYOS_API_KEY=your-key
```

## API

### `MemoryClient(url, api_key, timeout)`

| Parameter | Default | Description |
|-----------|---------|-------------|
| `url` | `$MEMORYOS_URL` or `http://localhost:8080` | Kernel base URL |
| `api_key` | `$MEMORYOS_API_KEY` or `""` | Management API key |
| `timeout` | `30.0` | HTTP timeout in seconds |

### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `list_memories(agent_id, limit, offset)` | `list[Memory]` | Paginated live memories |
| `create(agent_id, content, ...)` | `Memory` | Manually store a memory |
| `delete(memory_id)` | `None` | Hard-delete by ID |
| `search(agent_id, query, limit)` | `MemorySearchResult` | Semantic search |
| `restore_memory(memory_id)` | `Memory` | Un-tombstone a memory |
| `list_sessions(agent_id)` | `list[Session]` | Active sessions with L1 summaries |
| `get_session(agent_id, session_id)` | `Session` | Full L1 summary for one session |
| `delete_session(agent_id, session_id)` | `None` | Clear working memory |
| `list_archival_batches(agent_id)` | `list[ArchivalBatch]` | L2→L3 compaction history |
| `restore_batch(batch_id)` | `ArchivalBatch` | Restore entire batch atomically |
| `export_agent(agent_id, path)` | `str` | NDJSON export (writes file or returns string) |
| `import_agent(agent_id, path, ndjson)` | `ImportResult` | Bulk import with dedup |
| `stats()` | `dict` | Global agent and memory counts |

## Development

```bash
pip install -e ".[dev]"
pytest
```
