#!/usr/bin/env node
/**
 * AEON-IQ MCP Server
 *
 * Exposes MemoryOS Kernel's management API as MCP tools so Claude Desktop,
 * Cursor, Windsurf, and any MCP-compatible agent can query and store memories
 * without writing HTTP plumbing.
 *
 * Configuration (env vars):
 *   MEMORYOS_URL     - Kernel base URL (default: http://localhost:8080)
 *   MEMORYOS_API_KEY - Management API key (optional; required if kernel has one set)
 *   MEMORYOS_AGENT_ID - Default agent ID (required; callers can override per-tool)
 */

import { Server } from "@modelcontextprotocol/sdk/server/index.js";
import { StdioServerTransport } from "@modelcontextprotocol/sdk/server/stdio.js";
import {
  CallToolRequestSchema,
  ListToolsRequestSchema,
  Tool,
} from "@modelcontextprotocol/sdk/types.js";

// ── Config ────────────────────────────────────────────────────────────────────

const BASE_URL = (process.env.MEMORYOS_URL ?? "http://localhost:8080").replace(
  /\/+$/,
  ""
);
const API_KEY = process.env.MEMORYOS_API_KEY ?? "";
const DEFAULT_AGENT_ID = process.env.MEMORYOS_AGENT_ID ?? "";

function authHeaders(): Record<string, string> {
  const h: Record<string, string> = { "Content-Type": "application/json" };
  if (API_KEY) {
    h["X-Management-Key"] = API_KEY;
  }
  return h;
}

async function apiFetch(
  path: string,
  options: RequestInit = {}
): Promise<unknown> {
  const url = `${BASE_URL}${path}`;
  const res = await fetch(url, {
    ...options,
    headers: { ...authHeaders(), ...(options.headers as Record<string, string> ?? {}) },
  });

  const text = await res.text();
  if (!res.ok) {
    throw new Error(`MemoryOS API error ${res.status}: ${text}`);
  }

  try {
    return JSON.parse(text);
  } catch {
    return text;
  }
}

// ── Tool definitions ──────────────────────────────────────────────────────────

const TOOLS: Tool[] = [
  {
    name: "remember",
    description:
      "Store a new memory for an agent. Use this to persist facts, preferences, or decisions that should survive across sessions.",
    inputSchema: {
      type: "object",
      properties: {
        content: {
          type: "string",
          description: "The fact or information to remember.",
        },
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
        memory_type: {
          type: "string",
          enum: ["episodic", "semantic", "procedural"],
          description: "Memory type (default: semantic).",
        },
        importance: {
          type: "number",
          minimum: 0,
          maximum: 1,
          description: "Importance score 0.0–1.0 (default: 0.8).",
        },
      },
      required: ["content"],
    },
  },

  {
    name: "recall",
    description:
      "Search an agent's memories using semantic similarity. Returns the most relevant memories for the query.",
    inputSchema: {
      type: "object",
      properties: {
        query: {
          type: "string",
          description: "Natural language search query.",
        },
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
        limit: {
          type: "number",
          description: "Max results to return (default: 5).",
        },
      },
      required: ["query"],
    },
  },

  {
    name: "forget",
    description: "Hard-delete a specific memory by its ID.",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: {
          type: "string",
          description: "UUID of the memory to delete.",
        },
      },
      required: ["memory_id"],
    },
  },

  {
    name: "list_memories",
    description:
      "List all live (non-archived) memories for an agent, with optional pagination.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
        limit: {
          type: "number",
          description: "Page size (default: 20, max: 100).",
        },
        offset: {
          type: "number",
          description: "Pagination offset (default: 0).",
        },
      },
    },
  },

  {
    name: "get_sessions",
    description:
      "List all active sessions for an agent, including their L1 working-memory summaries.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
      },
    },
  },

  {
    name: "get_conflicts",
    description:
      "List unresolved memory conflicts for an agent (contradictory facts that have been flagged).",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
      },
    },
  },

  {
    name: "export_agent",
    description:
      "Export all memories for an agent as NDJSON. Returns the raw export data as a string.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description:
            "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
      },
    },
  },
];

// ── Tool handlers ─────────────────────────────────────────────────────────────

function agentId(args: Record<string, unknown>): string {
  const id = (args.agent_id as string | undefined) ?? DEFAULT_AGENT_ID;
  if (!id) {
    throw new Error(
      "agent_id is required. Pass it as a tool argument or set MEMORYOS_AGENT_ID."
    );
  }
  return id;
}

async function handleRemember(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const body = {
    content: args.content as string,
    memory_type: (args.memory_type as string | undefined) ?? "semantic",
    importance_score: (args.importance as number | undefined) ?? 0.8,
    importance_source: "user_stated",
    confidence: 0.95,
    provenance: "user_stated",
  };
  const result = await apiFetch(`/api/v1/agents/${encodeURIComponent(id)}/memories`, {
    method: "POST",
    body: JSON.stringify(body),
  });
  return JSON.stringify(result, null, 2);
}

async function handleRecall(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const body = {
    query: args.query as string,
    agent_id: id,
    limit: (args.limit as number | undefined) ?? 5,
  };
  const result = await apiFetch("/api/v1/memories/search", {
    method: "POST",
    body: JSON.stringify(body),
  });
  return JSON.stringify(result, null, 2);
}

async function handleForget(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  await apiFetch(`/api/v1/memories/${encodeURIComponent(memoryId)}`, {
    method: "DELETE",
  });
  return `Memory ${memoryId} deleted.`;
}

async function handleListMemories(
  args: Record<string, unknown>
): Promise<string> {
  const id = agentId(args);
  const limit = (args.limit as number | undefined) ?? 20;
  const offset = (args.offset as number | undefined) ?? 0;
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/memories?limit=${limit}&offset=${offset}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleGetSessions(
  args: Record<string, unknown>
): Promise<string> {
  const id = agentId(args);
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/sessions`
  );
  return JSON.stringify(result, null, 2);
}

async function handleGetConflicts(
  args: Record<string, unknown>
): Promise<string> {
  const id = agentId(args);
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/conflicts`
  );
  return JSON.stringify(result, null, 2);
}

async function handleExportAgent(
  args: Record<string, unknown>
): Promise<string> {
  const id = agentId(args);
  const url = `${BASE_URL}/api/v1/agents/${encodeURIComponent(id)}/export`;
  const res = await fetch(url, { headers: authHeaders() });
  if (!res.ok) {
    const text = await res.text();
    throw new Error(`Export failed ${res.status}: ${text}`);
  }
  return await res.text();
}

// ── Server bootstrap ──────────────────────────────────────────────────────────

const server = new Server(
  {
    name: "aeon-iq",
    version: "0.1.0",
  },
  {
    capabilities: {
      tools: {},
    },
  }
);

server.setRequestHandler(ListToolsRequestSchema, async () => ({
  tools: TOOLS,
}));

server.setRequestHandler(CallToolRequestSchema, async (request) => {
  const { name, arguments: args = {} } = request.params;
  const a = args as Record<string, unknown>;

  try {
    let text: string;

    switch (name) {
      case "remember":
        text = await handleRemember(a);
        break;
      case "recall":
        text = await handleRecall(a);
        break;
      case "forget":
        text = await handleForget(a);
        break;
      case "list_memories":
        text = await handleListMemories(a);
        break;
      case "get_sessions":
        text = await handleGetSessions(a);
        break;
      case "get_conflicts":
        text = await handleGetConflicts(a);
        break;
      case "export_agent":
        text = await handleExportAgent(a);
        break;
      default:
        throw new Error(`Unknown tool: ${name}`);
    }

    return {
      content: [{ type: "text", text }],
    };
  } catch (err) {
    const message = err instanceof Error ? err.message : String(err);
    return {
      content: [{ type: "text", text: `Error: ${message}` }],
      isError: true,
    };
  }
});

async function main(): Promise<void> {
  const transport = new StdioServerTransport();
  await server.connect(transport);
  process.stderr.write(
    `AEON-IQ MCP server running (kernel: ${BASE_URL})\n`
  );
}

main().catch((err) => {
  process.stderr.write(`Fatal: ${err}\n`);
  process.exit(1);
});
