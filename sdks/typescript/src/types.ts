/** Response types for the AEON-IQ management API. */

export interface Memory {
  id: string;
  agent_id?: string;
  session_id?: string;
  content: string;
  memory_type: string;
  confidence: number;
  provenance?: string;
  created_at: string;
  updated_at?: string;
  source_turn?: number;
  importance_score: number;
  importance_source?: string;
  status?: string;
  sensitivity?: string;
}

export interface MemorySearchResult {
  memories: Memory[];
  query: string;
}

export interface Session {
  session_id: string;
  agent_id: string;
  summary?: string;
  turn_count: number;
  updated_at: string;
}

export interface ArchivalBatch {
  id: string;
  agent_id: string;
  created_at: string;
  source_count: number;
  l3_count: number;
  status: "completed" | "restored" | "failed";
}

export interface ImportResult {
  imported: number;
  skipped_dedup: number;
  errors: number;
}

export interface Stats {
  agent_count: number;
  memory_count: number;
  tokens_saved_estimate: number;
}

export interface CreateMemoryOptions {
  memory_type?: string;
  importance?: number;
  confidence?: number;
  provenance?: string;
}

export interface ListMemoriesOptions {
  limit?: number;
  offset?: number;
}

export interface MemoryClientOptions {
  /** Kernel base URL. Defaults to `MEMORYOS_URL` env var or `http://localhost:8080`. */
  url?: string;
  /** Management API key. Defaults to `MEMORYOS_API_KEY` env var. */
  apiKey?: string;
  /** Fetch implementation. Defaults to the global `fetch`. */
  fetch?: typeof fetch;
}
