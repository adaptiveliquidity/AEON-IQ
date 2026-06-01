/**
 * MemoryClient — typed wrapper around the AEON-IQ management API.
 *
 * Uses the global `fetch` (Node.js 18+, browser, Deno, Bun).
 */

import type {
  ArchivalBatch,
  CreateMemoryOptions,
  ImportResult,
  ListMemoriesOptions,
  MemoryDiffResponse,
  Memory,
  MemoryClientOptions,
  MemorySearchResult,
  Session,
  Stats,
  TimeTravelResponse,
} from "./types.js";

export class MemoryClient {
  private readonly baseUrl: string;
  private readonly headers: Record<string, string>;
  private readonly fetchImpl: typeof fetch;

  constructor(options: MemoryClientOptions = {}) {
    const url =
      options.url ??
      (typeof process !== "undefined"
        ? process.env["MEMORYOS_URL"]
        : undefined) ??
      "http://localhost:8080";
    this.baseUrl = url.replace(/\/+$/, "");

    const key =
      options.apiKey ??
      (typeof process !== "undefined"
        ? process.env["MEMORYOS_API_KEY"]
        : undefined) ??
      "";

    this.headers = { "Content-Type": "application/json" };
    if (key) this.headers["X-Management-Key"] = key;

    this.fetchImpl = options.fetch ?? globalThis.fetch.bind(globalThis);
  }

  // ── Memories ──────────────────────────────────────────────────────────────

  /** Return paginated live memories for an agent. */
  async listMemories(
    agentId: string,
    options: ListMemoriesOptions = {}
  ): Promise<Memory[]> {
    const { limit = 50, offset = 0 } = options;
    const data = await this.get<{ memories: Memory[] }>(
      `/api/v1/agents/${enc(agentId)}/memories?limit=${limit}&offset=${offset}`
    );
    return data.memories ?? [];
  }

  /** Return memory state for an agent at a specific timestamp. */
  async timeTravel(
    agentId: string,
    timestamp: string,
    options: { limit?: number; offset?: number } = {}
  ): Promise<TimeTravelResponse> {
    const { limit = 50, offset = 0 } = options;
    const qs = new URLSearchParams({
      timestamp,
      limit: String(limit),
      offset: String(offset),
    });
    return this.get<TimeTravelResponse>(
      `/api/v1/agents/${enc(agentId)}/memories/at?${qs.toString()}`
    );
  }

  /** Return memory lifecycle diff between two timestamps. */
  async diff(agentId: string, fromTs: string, toTs: string): Promise<MemoryDiffResponse> {
    const qs = new URLSearchParams({ from: fromTs, to: toTs });
    return this.get<MemoryDiffResponse>(
      `/api/v1/agents/${enc(agentId)}/memories/diff?${qs.toString()}`
    );
  }

  /** Manually create a memory for an agent. */
  async create(
    agentId: string,
    content: string,
    options: CreateMemoryOptions = {}
  ): Promise<{ id: string; success: boolean }> {
    const body = {
      content,
      memory_type: options.memory_type ?? "semantic",
      importance_score: options.importance ?? 0.8,
      importance_source: "user_stated",
      confidence: options.confidence ?? 0.95,
      provenance: options.provenance ?? "user_stated",
    };
    return this.post<{ id: string; success: boolean }>(
      `/api/v1/agents/${enc(agentId)}/memories`,
      body
    );
  }

  /** Update memory content by ID. */
  async update(memoryId: string, content: string): Promise<{ updated: boolean }> {
    return this.patch<{ updated: boolean }>(`/api/v1/memories/${enc(memoryId)}`, {
      content,
    });
  }

  /** Hard-delete a memory by ID. */
  async delete(memoryId: string): Promise<void> {
    await this.del(`/api/v1/memories/${enc(memoryId)}`);
  }

  /** Semantic search over an agent's memories. */
  async search(
    agentId: string,
    query: string,
    limit = 5
  ): Promise<MemorySearchResult> {
    const data = await this.post<{ results?: Memory[]; memories?: Memory[] }>(
      "/api/v1/memories/search",
      { query, agent_id: agentId, limit }
    );
    return { memories: data.results ?? data.memories ?? [], query };
  }

  /** Restore a tombstoned memory. */
  async restoreMemory(memoryId: string): Promise<{ restored: boolean }> {
    return this.post<{ restored: boolean }>(
      `/api/v1/memories/${enc(memoryId)}/restore`,
      {}
    );
  }

  // ── Sessions ──────────────────────────────────────────────────────────────

  /** List all working-memory sessions for an agent. */
  async listSessions(agentId: string): Promise<Session[]> {
    const data = await this.get<{ sessions: Session[] }>(
      `/api/v1/agents/${enc(agentId)}/sessions`
    );
    return data.sessions ?? [];
  }

  /** Get the full L1 summary for a session. */
  async getSession(agentId: string, sessionId: string): Promise<Session> {
    return this.get<Session>(
      `/api/v1/agents/${enc(agentId)}/sessions/${enc(sessionId)}`
    );
  }

  /** Clear working memory for a session. */
  async deleteSession(agentId: string, sessionId: string): Promise<void> {
    await this.del(
      `/api/v1/agents/${enc(agentId)}/sessions/${enc(sessionId)}`
    );
  }

  // ── Archival ──────────────────────────────────────────────────────────────

  /** List all L2→L3 compaction batches for an agent. */
  async listArchivalBatches(agentId: string): Promise<ArchivalBatch[]> {
    const data = await this.get<{ batches: ArchivalBatch[] }>(
      `/api/v1/agents/${enc(agentId)}/archival/batches`
    );
    return data.batches ?? [];
  }

  /** Restore an entire archival batch (un-tombstone L2, re-tombstone L3). */
  async restoreBatch(
    batchId: string
  ): Promise<{ restored: boolean; batch_id: string; l2_restored: number; l3_tombstoned: number }> {
    return this.post<{ restored: boolean; batch_id: string; l2_restored: number; l3_tombstoned: number }>(
      `/api/v1/archival/batches/${enc(batchId)}/restore`,
      {}
    );
  }

  /** Trigger one archival run for an agent. */
  async triggerArchival(agentId: string): Promise<Record<string, unknown>> {
    return this.post<Record<string, unknown>>(
      `/api/v1/agents/${enc(agentId)}/archival/trigger`,
      {}
    );
  }

  /** List archived memories for an agent. */
  async listArchivedMemories(
    agentId: string,
    options: ListMemoriesOptions = {}
  ): Promise<Memory[]> {
    const { limit = 50, offset = 0 } = options;
    const data = await this.get<{ memories: Memory[] }>(
      `/api/v1/agents/${enc(agentId)}/memories/archived?limit=${limit}&offset=${offset}`
    );
    return data.memories ?? [];
  }

  /** Run bulk archive/delete for an agent. */
  async bulkOperation(
    agentId: string,
    action: "archive" | "delete",
    filter: Record<string, unknown>
  ): Promise<{ affected: number }> {
    return this.post<{ affected: number }>(
      `/api/v1/agents/${enc(agentId)}/memories/bulk`,
      { action, filter }
    );
  }

  // ── Export / Import ───────────────────────────────────────────────────────

  /** Export all memories as NDJSON string. */
  async exportAgent(agentId: string): Promise<string> {
    const res = await this.fetchImpl(
      `${this.baseUrl}/api/v1/agents/${enc(agentId)}/export`,
      { headers: this.headers }
    );
    await assertOk(res);
    return res.text();
  }

  /** Import memories from an NDJSON string. */
  async importAgent(agentId: string, ndjson: string): Promise<ImportResult> {
    const headers = { ...this.headers, "Content-Type": "application/x-ndjson" };
    const res = await this.fetchImpl(
      `${this.baseUrl}/api/v1/agents/${enc(agentId)}/import`,
      { method: "POST", headers, body: ndjson }
    );
    await assertOk(res);
    return res.json() as Promise<ImportResult>;
  }

  // ── Stats ─────────────────────────────────────────────────────────────────

  /** Return global memory counts and agent stats. */
  async stats(): Promise<Stats> {
    return this.get<Stats>("/api/v1/stats");
  }

  /** List all agents. */
  async listAgents(): Promise<{ agents: Array<{ agent_id: string; memory_count: number }>; total: number }> {
    return this.get<{ agents: Array<{ agent_id: string; memory_count: number }>; total: number }>(
      "/api/v1/agents"
    );
  }

  /** Delete an agent and all associated data. */
  async deleteAgent(agentId: string): Promise<void> {
    await this.del(`/api/v1/agents/${enc(agentId)}`);
  }

  /** List conflicts for an agent. */
  async listConflicts(agentId: string, includeResolved = false): Promise<Record<string, unknown>> {
    return this.get<Record<string, unknown>>(
      `/api/v1/agents/${enc(agentId)}/conflicts?include_resolved=${includeResolved}`
    );
  }

  /** Resolve a conflict. */
  async resolveConflict(conflictId: string, resolution: string): Promise<Record<string, unknown>> {
    return this.post<Record<string, unknown>>(
      `/api/v1/conflicts/${enc(conflictId)}/resolve`,
      { resolution }
    );
  }

  /** List retrieval logs for an agent. */
  async listRetrievals(
    agentId: string,
    options: { limit?: number; offset?: number; sessionId?: string } = {}
  ): Promise<Record<string, unknown>> {
    const { limit = 50, offset = 0, sessionId } = options;
    const qs = new URLSearchParams({
      limit: String(limit),
      offset: String(offset),
      ...(sessionId ? { session_id: sessionId } : {}),
    });
    return this.get<Record<string, unknown>>(
      `/api/v1/agents/${enc(agentId)}/retrievals?${qs.toString()}`
    );
  }

  /** List all versions for a memory. */
  async listMemoryVersions(memoryId: string): Promise<Record<string, unknown>> {
    return this.get<Record<string, unknown>>(`/api/v1/memories/${enc(memoryId)}/versions`);
  }

  /** Patch memory status. */
  async patchMemoryStatus(memoryId: string, status: string, reason?: string): Promise<Record<string, unknown>> {
    return this.patch<Record<string, unknown>>(`/api/v1/memories/${enc(memoryId)}/status`, {
      status,
      reason,
    });
  }

  /** Patch memory sensitivity. */
  async patchMemorySensitivity(memoryId: string, sensitivity: string): Promise<Record<string, unknown>> {
    return this.patch<Record<string, unknown>>(
      `/api/v1/memories/${enc(memoryId)}/sensitivity`,
      { sensitivity }
    );
  }

  /** Submit retrieval feedback. */
  async postFeedback(agentId: string, memoryId: string, feedback: number): Promise<{ recorded: boolean }> {
    return this.post<{ recorded: boolean }>("/api/v1/feedback", {
      agent_id: agentId,
      memory_id: memoryId,
      feedback,
    });
  }

  // ── Private helpers ───────────────────────────────────────────────────────

  private async get<T>(path: string): Promise<T> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      headers: this.headers,
    });
    await assertOk(res);
    return res.json() as Promise<T>;
  }

  private async post<T>(path: string, body: unknown): Promise<T> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: "POST",
      headers: this.headers,
      body: JSON.stringify(body),
    });
    await assertOk(res);
    if (res.status === 204) return {} as T;
    return res.json() as Promise<T>;
  }

  private async del(path: string): Promise<void> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: "DELETE",
      headers: this.headers,
    });
    await assertOk(res);
  }

  private async patch<T>(path: string, body: unknown): Promise<T> {
    const res = await this.fetchImpl(`${this.baseUrl}${path}`, {
      method: "PATCH",
      headers: this.headers,
      body: JSON.stringify(body),
    });
    await assertOk(res);
    if (res.status === 204) return {} as T;
    return res.json() as Promise<T>;
  }
}

function enc(s: string): string {
  return encodeURIComponent(s);
}

async function assertOk(res: Response): Promise<void> {
  if (!res.ok) {
    let detail: unknown;
    try {
      detail = await res.clone().json();
    } catch {
      detail = await res.text();
    }
    throw new Error(`MemoryOS API error ${res.status}: ${JSON.stringify(detail)}`);
  }
}
