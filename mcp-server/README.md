# @aeon-iq/mcp-server

MCP server that wraps [AEON-IQ MemoryOS](../README.md), exposing persistent agent memory as tools for Claude Desktop, Cursor, Windsurf, and any MCP-compatible agent.

## Installation

```bash
npm install -g @aeon-iq/mcp-server
# or run without installing:
npx @aeon-iq/mcp-server
```

## Configuration

| Env var | Default | Description |
|---------|---------|-------------|
| `MEMORYOS_URL` | `http://localhost:8080` | MemoryOS Kernel base URL |
| `MEMORYOS_API_KEY` | *(empty)* | Management API key (required if kernel has one set) |
| `MEMORYOS_AGENT_ID` | *(required)* | Default agent ID used when tools don't specify one |

## Claude Desktop setup

Add to `~/.claude/claude_desktop_config.json`:

```json
{
  "mcpServers": {
    "aeon-iq": {
      "command": "npx",
      "args": ["@aeon-iq/mcp-server"],
      "env": {
        "MEMORYOS_URL": "http://localhost:8080",
        "MEMORYOS_API_KEY": "your-key-here",
        "MEMORYOS_AGENT_ID": "claude-desktop"
      }
    }
  }
}
```

## Available tools

| Tool | Description |
|------|-------------|
| `remember` | Store a new memory (content, type, importance) |
| `recall` | Semantic search over memories |
| `forget` | Hard-delete a memory by ID |
| `list_memories` | Paginated list of live memories |
| `get_sessions` | List sessions with L1 working-memory summaries |
| `get_conflicts` | List unresolved contradictions between memories |
| `export_agent` | Export all memories as NDJSON |

## Development

```bash
npm install
npm run build    # compiles TypeScript → dist/
npm run dev      # watch mode
npm start        # run the compiled server
```
