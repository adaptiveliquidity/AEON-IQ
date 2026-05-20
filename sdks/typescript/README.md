# @aeon-iq/client

TypeScript client SDK for [AEON-IQ MemoryOS](../../README.md) — persistent memory for AI agents.

## Installation

```bash
npm install @aeon-iq/client
```

## Quick start

```typescript
import { MemoryClient } from "@aeon-iq/client";

const client = new MemoryClient({
  url: "http://localhost:8080",
  apiKey: "your-management-api-key",
});

// Store a fact
await client.create("my-bot", "User prefers dark mode", { importance: 0.8 });

// Semantic search
const results = await client.search("my-bot", "user preferences", 5);
for (const m of results.memories) {
  console.log(m.content, m.importance_score);
}

// Export to NDJSON
const ndjson = await client.exportAgent("my-bot");
```

## Configuration

```typescript
const client = new MemoryClient({
  url: process.env.MEMORYOS_URL ?? "http://localhost:8080",
  apiKey: process.env.MEMORYOS_API_KEY,
});
```

Environment variables `MEMORYOS_URL` and `MEMORYOS_API_KEY` are read automatically when running in Node.js.

## API

### `new MemoryClient(options?)`

| Option | Default | Description |
|--------|---------|-------------|
| `url` | `$MEMORYOS_URL` or `http://localhost:8080` | Kernel base URL |
| `apiKey` | `$MEMORYOS_API_KEY` or `""` | Management API key |
| `fetch` | `globalThis.fetch` | Custom fetch implementation |

### Methods

| Method | Returns | Description |
|--------|---------|-------------|
| `listMemories(agentId, options?)` | `Promise<Memory[]>` | Paginated live memories |
| `create(agentId, content, options?)` | `Promise<Memory>` | Manually store a memory |
| `delete(memoryId)` | `Promise<void>` | Hard-delete by ID |
| `search(agentId, query, limit?)` | `Promise<MemorySearchResult>` | Semantic search |
| `restoreMemory(memoryId)` | `Promise<Memory>` | Un-tombstone a memory |
| `listSessions(agentId)` | `Promise<Session[]>` | Active sessions with L1 summaries |
| `getSession(agentId, sessionId)` | `Promise<Session>` | Full L1 summary for one session |
| `deleteSession(agentId, sessionId)` | `Promise<void>` | Clear working memory |
| `listArchivalBatches(agentId)` | `Promise<ArchivalBatch[]>` | L2→L3 compaction history |
| `restoreBatch(batchId)` | `Promise<ArchivalBatch>` | Restore entire batch atomically |
| `exportAgent(agentId)` | `Promise<string>` | NDJSON export |
| `importAgent(agentId, ndjson)` | `Promise<ImportResult>` | Bulk import with dedup |
| `stats()` | `Promise<Stats>` | Global agent and memory counts |

## Development

```bash
npm install
npm run build
```
