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
  {
    name: "list_agents",
    description: "List all known agents with their memory counts.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "get_stats",
    description: "Get global memory and agent statistics.",
    inputSchema: {
      type: "object",
      properties: {},
    },
  },
  {
    name: "get_retrievals",
    description: "List retrieval logs for an agent.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description: "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
        limit: {
          type: "number",
          description: "Page size (default: 50).",
        },
        offset: {
          type: "number",
          description: "Pagination offset (default: 0).",
        },
        session_id: {
          type: "string",
          description: "Optional session filter.",
        },
      },
    },
  },
  {
    name: "time_travel",
    description: "Return memory state for an agent at a specific timestamp.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        timestamp: { type: "string", description: "ISO-8601 timestamp." },
        limit: { type: "number", description: "Page size (default: 50)." },
        offset: { type: "number", description: "Pagination offset (default: 0)." },
      },
      required: ["timestamp"],
    },
  },
  {
    name: "memory_diff",
    description: "Return memory changes for an agent between two timestamps.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        from: { type: "string", description: "Start ISO-8601 timestamp." },
        to: { type: "string", description: "End ISO-8601 timestamp." },
      },
      required: ["from", "to"],
    },
  },
  {
    name: "get_versions",
    description: "Get full version history for a memory.",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: {
          type: "string",
          description: "UUID of the memory.",
        },
      },
      required: ["memory_id"],
    },
  },
  {
    name: "set_status",
    description: "Update memory lifecycle status (active/candidate/quarantined/suppressed).",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: {
          type: "string",
          description: "UUID of the memory.",
        },
        status: {
          type: "string",
          enum: ["active", "candidate", "quarantined", "suppressed"],
          description: "Target lifecycle status.",
        },
        reason: {
          type: "string",
          description: "Optional reason (used for suppressed status).",
        },
      },
      required: ["memory_id", "status"],
    },
  },
  {
    name: "submit_feedback",
    description: "Submit retrieval-quality feedback for a memory (0.0-1.0).",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description: "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
        memory_id: {
          type: "string",
          description: "UUID of the memory being rated.",
        },
        feedback: {
          type: "number",
          minimum: 0,
          maximum: 1,
          description: "Feedback score between 0.0 and 1.0.",
        },
      },
      required: ["memory_id", "feedback"],
    },
  },
  {
    name: "delete_agent",
    description: "Delete an agent and all associated memory data.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: {
          type: "string",
          description: "Agent identifier. Defaults to MEMORYOS_AGENT_ID env var.",
        },
      },
    },
  },
  {
    name: "patch_memory",
    description: "Update memory content by ID.",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: { type: "string", description: "Memory UUID." },
        content: { type: "string", description: "Updated memory content." },
      },
      required: ["memory_id", "content"],
    },
  },
  {
    name: "restore_memory",
    description: "Restore a tombstoned memory by ID.",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: { type: "string", description: "Memory UUID." },
      },
      required: ["memory_id"],
    },
  },
  {
    name: "list_archived_memories",
    description: "List archived memories for an agent.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        limit: { type: "number", description: "Page size (default 50)." },
        offset: { type: "number", description: "Pagination offset (default 0)." },
      },
    },
  },
  {
    name: "list_archival_batches",
    description: "List archival batches for an agent.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        limit: { type: "number", description: "Page size (default 50)." },
        offset: { type: "number", description: "Pagination offset (default 0)." },
      },
    },
  },
  {
    name: "restore_archival_batch",
    description: "Restore a full archival batch by batch ID.",
    inputSchema: {
      type: "object",
      properties: {
        batch_id: { type: "string", description: "Archival batch UUID." },
      },
      required: ["batch_id"],
    },
  },
  {
    name: "trigger_archival",
    description: "Trigger one archival compaction run for an agent.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
      },
    },
  },
  {
    name: "bulk_memories",
    description: "Run bulk archive/delete over filtered memories.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        action: { type: "string", enum: ["archive", "delete"] },
        filter: { type: "object", description: "Bulk filter object." },
      },
      required: ["action"],
    },
  },
  {
    name: "import_agent",
    description: "Import an agent NDJSON export payload.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        ndjson: { type: "string", description: "NDJSON content to import." },
      },
      required: ["ndjson"],
    },
  },
  {
    name: "get_session",
    description: "Get one session detail by session ID.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        session_id: { type: "string", description: "Session ID." },
      },
      required: ["session_id"],
    },
  },
  {
    name: "delete_session",
    description: "Delete one session working memory by session ID.",
    inputSchema: {
      type: "object",
      properties: {
        agent_id: { type: "string", description: "Agent identifier." },
        session_id: { type: "string", description: "Session ID." },
      },
      required: ["session_id"],
    },
  },
  {
    name: "resolve_conflict",
    description: "Resolve a conflict (keep_a, keep_b, keep_both, dismissed).",
    inputSchema: {
      type: "object",
      properties: {
        conflict_id: { type: "string", description: "Conflict UUID." },
        resolution: {
          type: "string",
          enum: ["keep_a", "keep_b", "keep_both", "dismissed"],
        },
      },
      required: ["conflict_id", "resolution"],
    },
  },
  {
    name: "set_sensitivity",
    description: "Set memory sensitivity classification.",
    inputSchema: {
      type: "object",
      properties: {
        memory_id: { type: "string", description: "Memory UUID." },
        sensitivity: {
          type: "string",
          enum: ["unknown", "normal", "private", "sensitive", "secret"],
        },
      },
      required: ["memory_id", "sensitivity"],
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
  const includeResolved = (args.include_resolved as boolean | undefined) ?? false;
  const qs = new URLSearchParams({ include_resolved: String(includeResolved) });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/conflicts?${qs.toString()}`
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

async function handleListAgents(): Promise<string> {
  const result = await apiFetch("/api/v1/agents");
  return JSON.stringify(result, null, 2);
}

async function handleGetStats(): Promise<string> {
  const result = await apiFetch("/api/v1/stats");
  return JSON.stringify(result, null, 2);
}

async function handleGetRetrievals(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const limit = (args.limit as number | undefined) ?? 50;
  const offset = (args.offset as number | undefined) ?? 0;
  const sessionId = args.session_id as string | undefined;
  const qs = new URLSearchParams({
    limit: String(limit),
    offset: String(offset),
    ...(sessionId ? { session_id: sessionId } : {}),
  });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/retrievals?${qs.toString()}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleTimeTravel(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const timestamp = args.timestamp as string;
  const limit = (args.limit as number | undefined) ?? 50;
  const offset = (args.offset as number | undefined) ?? 0;
  const qs = new URLSearchParams({
    timestamp,
    limit: String(limit),
    offset: String(offset),
  });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/memories/at?${qs.toString()}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleMemoryDiff(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const from = args.from as string;
  const to = args.to as string;
  const qs = new URLSearchParams({ from, to });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/memories/diff?${qs.toString()}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleGetVersions(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  const result = await apiFetch(`/api/v1/memories/${encodeURIComponent(memoryId)}/versions`);
  return JSON.stringify(result, null, 2);
}

async function handleSetStatus(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  const status = args.status as string;
  const reason = args.reason as string | undefined;
  const result = await apiFetch(`/api/v1/memories/${encodeURIComponent(memoryId)}/status`, {
    method: "PATCH",
    body: JSON.stringify({ status, reason }),
  });
  return JSON.stringify(result, null, 2);
}

async function handleSubmitFeedback(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const body = {
    agent_id: id,
    memory_id: args.memory_id as string,
    feedback: (args.feedback as number | undefined) ?? 1.0,
  };
  const result = await apiFetch("/api/v1/feedback", {
    method: "POST",
    body: JSON.stringify(body),
  });
  return JSON.stringify(result, null, 2);
}

async function handleDeleteAgent(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  await apiFetch(`/api/v1/agents/${encodeURIComponent(id)}`, { method: "DELETE" });
  return `Agent ${id} deleted.`;
}

async function handlePatchMemory(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  const content = args.content as string;
  const result = await apiFetch(`/api/v1/memories/${encodeURIComponent(memoryId)}`, {
    method: "PATCH",
    body: JSON.stringify({ content }),
  });
  return JSON.stringify(result, null, 2);
}

async function handleRestoreMemory(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  const result = await apiFetch(`/api/v1/memories/${encodeURIComponent(memoryId)}/restore`, {
    method: "POST",
    body: JSON.stringify({}),
  });
  return JSON.stringify(result, null, 2);
}

async function handleListArchivedMemories(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const limit = (args.limit as number | undefined) ?? 50;
  const offset = (args.offset as number | undefined) ?? 0;
  const qs = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/memories/archived?${qs.toString()}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleListArchivalBatches(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const limit = (args.limit as number | undefined) ?? 50;
  const offset = (args.offset as number | undefined) ?? 0;
  const qs = new URLSearchParams({ limit: String(limit), offset: String(offset) });
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/archival/batches?${qs.toString()}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleRestoreArchivalBatch(args: Record<string, unknown>): Promise<string> {
  const batchId = args.batch_id as string;
  const result = await apiFetch(
    `/api/v1/archival/batches/${encodeURIComponent(batchId)}/restore`,
    {
      method: "POST",
      body: JSON.stringify({}),
    }
  );
  return JSON.stringify(result, null, 2);
}

async function handleTriggerArchival(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/archival/trigger`,
    {
      method: "POST",
      body: JSON.stringify({}),
    }
  );
  return JSON.stringify(result, null, 2);
}

async function handleBulkMemories(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const action = (args.action as string | undefined) ?? "archive";
  const filter = (args.filter as Record<string, unknown> | undefined) ?? {};
  const result = await apiFetch(`/api/v1/agents/${encodeURIComponent(id)}/memories/bulk`, {
    method: "POST",
    body: JSON.stringify({ action, filter }),
  });
  return JSON.stringify(result, null, 2);
}

async function handleImportAgent(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const ndjson = args.ndjson as string;
  const result = await apiFetch(`/api/v1/agents/${encodeURIComponent(id)}/import`, {
    method: "POST",
    headers: { ...authHeaders(), "Content-Type": "application/x-ndjson" },
    body: ndjson,
  });
  return JSON.stringify(result, null, 2);
}

async function handleGetSession(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const sessionId = args.session_id as string;
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/sessions/${encodeURIComponent(sessionId)}`
  );
  return JSON.stringify(result, null, 2);
}

async function handleDeleteSession(args: Record<string, unknown>): Promise<string> {
  const id = agentId(args);
  const sessionId = args.session_id as string;
  const result = await apiFetch(
    `/api/v1/agents/${encodeURIComponent(id)}/sessions/${encodeURIComponent(sessionId)}`,
    { method: "DELETE" }
  );
  return JSON.stringify(result, null, 2);
}

async function handleResolveConflict(args: Record<string, unknown>): Promise<string> {
  const conflictId = args.conflict_id as string;
  const resolution = args.resolution as string;
  const result = await apiFetch(
    `/api/v1/conflicts/${encodeURIComponent(conflictId)}/resolve`,
    {
      method: "POST",
      body: JSON.stringify({ resolution }),
    }
  );
  return JSON.stringify(result, null, 2);
}

async function handleSetSensitivity(args: Record<string, unknown>): Promise<string> {
  const memoryId = args.memory_id as string;
  const sensitivity = args.sensitivity as string;
  const result = await apiFetch(
    `/api/v1/memories/${encodeURIComponent(memoryId)}/sensitivity`,
    {
      method: "PATCH",
      body: JSON.stringify({ sensitivity }),
    }
  );
  return JSON.stringify(result, null, 2);
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
      case "list_agents":
        text = await handleListAgents();
        break;
      case "get_stats":
        text = await handleGetStats();
        break;
      case "get_retrievals":
        text = await handleGetRetrievals(a);
        break;
      case "time_travel":
        text = await handleTimeTravel(a);
        break;
      case "memory_diff":
        text = await handleMemoryDiff(a);
        break;
      case "get_versions":
        text = await handleGetVersions(a);
        break;
      case "set_status":
        text = await handleSetStatus(a);
        break;
      case "submit_feedback":
        text = await handleSubmitFeedback(a);
        break;
      case "delete_agent":
        text = await handleDeleteAgent(a);
        break;
      case "patch_memory":
        text = await handlePatchMemory(a);
        break;
      case "restore_memory":
        text = await handleRestoreMemory(a);
        break;
      case "list_archived_memories":
        text = await handleListArchivedMemories(a);
        break;
      case "list_archival_batches":
        text = await handleListArchivalBatches(a);
        break;
      case "restore_archival_batch":
        text = await handleRestoreArchivalBatch(a);
        break;
      case "trigger_archival":
        text = await handleTriggerArchival(a);
        break;
      case "bulk_memories":
        text = await handleBulkMemories(a);
        break;
      case "import_agent":
        text = await handleImportAgent(a);
        break;
      case "get_session":
        text = await handleGetSession(a);
        break;
      case "delete_session":
        text = await handleDeleteSession(a);
        break;
      case "resolve_conflict":
        text = await handleResolveConflict(a);
        break;
      case "set_sensitivity":
        text = await handleSetSensitivity(a);
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
