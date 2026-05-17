"use client";

import { useEffect, useState, useCallback } from "react";
import {
  Search,
  Trash2,
  Plus,
  RefreshCw,
  Brain,
  GitBranch,
  List,
  Lock,
} from "lucide-react";

// ── Types ─────────────────────────────────────────────────────────────────────

interface MemoryDto {
  id: string;
  content: string;
  memory_type: string;
  confidence: number;
  provenance: string;
  created_at: string;
  session_id: string | null;
  source_turn: number | null;
  importance_score: number;
  importance_source: string;
}

interface SearchResult extends MemoryDto {
  similarity: number;
  importance_score: number;
}

interface RelationDto {
  id: string;
  subject: string;
  predicate: string;
  object: string;
  confidence: number;
  created_at: string;
}

interface MemoryList {
  memories: MemoryDto[];
  total: number;
}

interface SearchResponse {
  results: SearchResult[];
  relations: RelationDto[];
  total: number;
}

// ── Constants ─────────────────────────────────────────────────────────────────

const TYPE_COLORS: Record<string, string> = {
  episodic:   "bg-blue-500/20 text-blue-300 border-blue-500/30",
  semantic:   "bg-green-500/20 text-green-300 border-green-500/30",
  procedural: "bg-yellow-500/20 text-yellow-300 border-yellow-500/30",
};

const PROV_COLORS: Record<string, string> = {
  user_stated:       "text-green-400",
  assistant_derived: "text-yellow-400",
  inferred:          "text-zinc-400",
};

const TABS = ["browse", "search", "graph"] as const;
type Tab = (typeof TABS)[number];

// ── Component ─────────────────────────────────────────────────────────────────

export default function MemoryExplorerClient({
  userAgentId,
  isAdmin,
}: {
  userAgentId: string;
  isAdmin: boolean;
}) {
  const [agentId, setAgentId]           = useState(userAgentId);
  const [tab, setTab]                   = useState<Tab>("browse");

  // Browse tab
  const [browseData, setBrowseData]     = useState<MemoryList | null>(null);
  const [browseFilter, setBrowseFilter] = useState("");
  const [adding, setAdding]             = useState(false);
  const [newContent, setNewContent]     = useState("");
  const [newType, setNewType]           = useState("semantic");

  // Semantic search tab
  const [query, setQuery]               = useState("");
  const [threshold, setThreshold]       = useState("0.80");
  const [searchData, setSearchData]     = useState<SearchResponse | null>(null);
  const [searching, setSearching]       = useState(false);

  const [loading, setLoading]           = useState(false);
  const [error, setError]               = useState<string | null>(null);

  // Keep agentId in sync if the session's userAgentId changes (e.g. after navigation).
  useEffect(() => {
    if (!isAdmin) setAgentId(userAgentId);
  }, [userAgentId, isAdmin]);

  // ── Data fetchers ──────────────────────────────────────────────────────────

  const loadBrowse = useCallback(async () => {
    if (!agentId.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(
        `/api/agents/${encodeURIComponent(agentId)}/memories`
      );
      if (!res.ok) throw new Error(await res.text());
      setBrowseData(await res.json());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [agentId]);

  useEffect(() => {
    const t = setTimeout(() => {
      if (agentId && tab === "browse") loadBrowse();
    }, 400);
    return () => clearTimeout(t);
  }, [agentId, tab, loadBrowse]);

  const handleSemanticSearch = async () => {
    if (!agentId.trim() || !query.trim()) return;
    setSearching(true);
    setError(null);
    try {
      const res = await fetch(
        `/api/agents/${encodeURIComponent(agentId)}/memories/search`,
        {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify({
            query,
            limit: 20,
            threshold: parseFloat(threshold) || 0.8,
            include_relations: tab === "graph",
          }),
        }
      );
      if (!res.ok) throw new Error(await res.text());
      setSearchData(await res.json());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setSearching(false);
    }
  };

  // ── CRUD ──────────────────────────────────────────────────────────────────

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this memory?")) return;
    await fetch(`/api/memories/${id}`, { method: "DELETE" });
    loadBrowse();
  };

  const handleAdd = async () => {
    if (!newContent.trim() || !agentId.trim()) return;
    setLoading(true);
    try {
      await fetch(`/api/agents/${encodeURIComponent(agentId)}/memories`, {
        method: "POST",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ content: newContent, memory_type: newType }),
      });
      setNewContent("");
      setAdding(false);
      loadBrowse();
    } finally {
      setLoading(false);
    }
  };

  // ── Derived ───────────────────────────────────────────────────────────────

  const filteredBrowse =
    browseData?.memories.filter(
      (m) =>
        !browseFilter ||
        m.content.toLowerCase().includes(browseFilter.toLowerCase())
    ) ?? [];

  const relations = searchData?.relations ?? [];

  // ── Render ────────────────────────────────────────────────────────────────

  return (
    <div className="space-y-6">

      {/* Header */}
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Memory Explorer</h1>
          <p className="text-zinc-400 text-sm mt-1">
            Browse, search, and inspect per-agent memories
          </p>
        </div>
      </div>

      {/* Agent ID input — locked for non-admins */}
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4">
        <div className="flex items-center justify-between mb-1">
          <label className="text-xs text-zinc-400">Agent ID</label>
          {!isAdmin && (
            <span className="flex items-center gap-1 text-xs text-zinc-600">
              <Lock className="w-3 h-3" />
              scoped to your account
            </span>
          )}
        </div>
        <input
          value={agentId}
          onChange={isAdmin ? (e) => setAgentId(e.target.value) : undefined}
          readOnly={!isAdmin}
          placeholder="e.g. my-agent-001"
          className={`w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm focus:outline-none ${
            isAdmin
              ? "focus:ring-1 focus:ring-green-500"
              : "cursor-default opacity-70 select-all"
          }`}
        />
      </div>

      {/* Tabs */}
      <div className="flex gap-1 border-b border-zinc-800 pb-0">
        {(
          [
            { id: "browse", label: "All Memories",    icon: List },
            { id: "search", label: "Semantic Search",  icon: Brain },
            { id: "graph",  label: "Knowledge Graph",  icon: GitBranch },
          ] as const
        ).map(({ id, label, icon: Icon }) => (
          <button
            key={id}
            onClick={() => setTab(id)}
            className={`flex items-center gap-1.5 px-4 py-2 text-sm rounded-t-lg transition-colors ${
              tab === id
                ? "bg-zinc-900 border border-b-zinc-900 border-zinc-800 text-zinc-100 -mb-px"
                : "text-zinc-500 hover:text-zinc-300"
            }`}
          >
            <Icon className="w-3.5 h-3.5" />
            {label}
          </button>
        ))}
      </div>

      {/* Error banner */}
      {error && (
        <div className="rounded-xl border border-red-600/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* ── Browse tab ──────────────────────────────────────────────────── */}
      {tab === "browse" && (
        <div className="space-y-4">
          <div className="flex gap-3">
            <div className="relative flex-1">
              <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-zinc-500" />
              <input
                value={browseFilter}
                onChange={(e) => setBrowseFilter(e.target.value)}
                placeholder="Filter content (client-side)…"
                className="w-full bg-zinc-900 border border-zinc-800 rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500"
              />
            </div>
            <button
              onClick={loadBrowse}
              disabled={loading || !agentId}
              className="flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 transition-colors"
            >
              <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
              Refresh
            </button>
            <button
              onClick={() => setAdding(!adding)}
              disabled={!agentId}
              className="flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 transition-colors font-medium"
            >
              <Plus className="w-3.5 h-3.5" />
              Add
            </button>
          </div>

          {/* Add form */}
          {adding && (
            <div className="rounded-xl border border-green-600/30 bg-zinc-900 p-4 space-y-3">
              <h3 className="text-sm font-semibold text-green-400">New Memory</h3>
              <textarea
                value={newContent}
                onChange={(e) => setNewContent(e.target.value)}
                rows={3}
                placeholder="Enter memory content…"
                className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500 resize-none"
              />
              <div className="flex items-center gap-3">
                <select
                  value={newType}
                  onChange={(e) => setNewType(e.target.value)}
                  className="bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm"
                >
                  <option value="semantic">semantic</option>
                  <option value="episodic">episodic</option>
                  <option value="procedural">procedural</option>
                </select>
                <button
                  onClick={handleAdd}
                  disabled={!newContent.trim()}
                  className="px-4 py-1.5 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 font-medium"
                >
                  Save
                </button>
                <button
                  onClick={() => setAdding(false)}
                  className="px-4 py-1.5 text-sm rounded-lg border border-zinc-700 hover:bg-zinc-800"
                >
                  Cancel
                </button>
              </div>
            </div>
          )}

          <MemoryTable
            memories={filteredBrowse}
            total={browseData?.total}
            loaded={filteredBrowse.length}
            agentId={agentId}
            loading={loading}
            onDelete={handleDelete}
          />
        </div>
      )}

      {/* ── Semantic Search tab ──────────────────────────────────────────── */}
      {tab === "search" && (
        <div className="space-y-4">
          <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 space-y-3">
            <label className="block text-xs text-zinc-400">Natural language query</label>
            <div className="flex gap-3">
              <div className="relative flex-1">
                <Brain className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-zinc-500" />
                <input
                  value={query}
                  onChange={(e) => setQuery(e.target.value)}
                  onKeyDown={(e) => e.key === "Enter" && handleSemanticSearch()}
                  placeholder="e.g. what startup is the user building?"
                  className="w-full bg-zinc-800 border border-zinc-700 rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500"
                />
              </div>
              <div className="flex items-center gap-1.5">
                <span className="text-xs text-zinc-500">threshold</span>
                <input
                  type="number"
                  value={threshold}
                  onChange={(e) => setThreshold(e.target.value)}
                  min="0"
                  max="1"
                  step="0.05"
                  className="w-20 bg-zinc-800 border border-zinc-700 rounded-lg px-2 py-2 text-sm text-center focus:outline-none"
                />
              </div>
              <button
                onClick={handleSemanticSearch}
                disabled={searching || !agentId || !query}
                className="flex items-center gap-1.5 px-4 py-2 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 font-medium transition-colors"
              >
                <Search className={`w-3.5 h-3.5 ${searching ? "animate-pulse" : ""}`} />
                Search
              </button>
            </div>
            <p className="text-xs text-zinc-600">
              Embeds your query and runs pgvector HNSW cosine similarity against the
              agent&apos;s memories. Lower threshold = stricter match (0 = identical, 1 = any).
            </p>
          </div>

          {searchData && (
            <div className="space-y-3">
              <p className="text-sm text-zinc-400">
                {searchData.total} result{searchData.total !== 1 ? "s" : ""} ranked by similarity
              </p>
              {searchData.results.length === 0 ? (
                <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-10 text-center text-zinc-500 text-sm">
                  No memories matched. Try raising the threshold or rephrasing the query.
                </div>
              ) : (
                <div className="rounded-xl border border-zinc-800 bg-zinc-900 divide-y divide-zinc-800">
                  {searchData.results.map((r) => (
                    <div key={r.id} className="px-5 py-4 space-y-1.5">
                      <div className="flex items-start justify-between gap-4">
                        <p className="text-sm text-zinc-200 leading-relaxed flex-1">{r.content}</p>
                        <div className="shrink-0 text-right">
                          <div className="text-lg font-bold text-green-400">
                            {(r.similarity * 100).toFixed(0)}%
                          </div>
                          <div className="text-xs text-zinc-600">similarity</div>
                        </div>
                      </div>
                      <div className="flex flex-wrap gap-2">
                        <span className={`text-xs px-2 py-0.5 rounded-full border ${TYPE_COLORS[r.memory_type] ?? "bg-zinc-700 text-zinc-300 border-zinc-600"}`}>
                          {r.memory_type}
                        </span>
                        <span className={`text-xs ${PROV_COLORS[r.provenance] ?? "text-zinc-500"}`}>
                          {r.provenance}
                        </span>
                        <span className="text-xs text-zinc-500">
                          conf: {(r.confidence * 100).toFixed(0)}%
                        </span>
                        <ImportanceBadge score={r.importance_score} source={r.importance_source} />
                        {r.source_turn !== null && (
                          <span className="text-xs text-zinc-500">turn {r.source_turn}</span>
                        )}
                        <span className="text-xs text-zinc-600">
                          {new Date(r.created_at).toLocaleString()}
                        </span>
                      </div>
                    </div>
                  ))}
                </div>
              )}
            </div>
          )}
        </div>
      )}

      {/* ── Knowledge Graph tab ──────────────────────────────────────────── */}
      {tab === "graph" && (
        <div className="space-y-4">
          <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 flex gap-3">
            <div className="relative flex-1">
              <Brain className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-zinc-500" />
              <input
                value={query}
                onChange={(e) => setQuery(e.target.value)}
                onKeyDown={(e) => e.key === "Enter" && handleSemanticSearch()}
                placeholder="Query to anchor the graph (optional)…"
                className="w-full bg-zinc-800 border border-zinc-700 rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500"
              />
            </div>
            <button
              onClick={handleSemanticSearch}
              disabled={searching || !agentId}
              className="flex items-center gap-1.5 px-4 py-2 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 font-medium"
            >
              <GitBranch className={`w-3.5 h-3.5 ${searching ? "animate-pulse" : ""}`} />
              Load Graph
            </button>
          </div>

          {relations.length > 0 ? (
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden">
              <div className="px-5 py-3 border-b border-zinc-800 flex items-center justify-between">
                <h3 className="font-semibold text-sm">
                  Knowledge Graph — {relations.length} relations
                </h3>
                <span className="text-xs text-zinc-500">subject → predicate → object</span>
              </div>
              <div className="divide-y divide-zinc-800 max-h-[600px] overflow-y-auto">
                {relations.map((r) => (
                  <div key={r.id} className="px-5 py-3 flex items-center gap-3 text-sm hover:bg-zinc-800/40">
                    <span className="font-medium text-blue-300 min-w-[120px] truncate">{r.subject}</span>
                    <span className="text-xs text-zinc-500 px-2 py-0.5 bg-zinc-800 rounded-full whitespace-nowrap">
                      {r.predicate}
                    </span>
                    <span className="font-medium text-green-300 min-w-[120px] truncate">{r.object}</span>
                    <span className="ml-auto text-xs text-zinc-600 shrink-0">
                      conf: {(r.confidence * 100).toFixed(0)}%
                    </span>
                  </div>
                ))}
              </div>
            </div>
          ) : searchData !== null ? (
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
              No relations extracted yet for this agent.
              Relations are built automatically as conversations happen.
            </div>
          ) : (
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
              Click &quot;Load Graph&quot; to fetch the knowledge graph for this agent.
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── ImportanceBadge sub-component ────────────────────────────────────────────

function ImportanceBadge({ score, source }: { score: number; source: string }) {
  const pct = Math.round(score * 100);
  const color =
    score >= 0.9  ? "text-rose-400"   :
    score >= 0.7  ? "text-orange-400" :
    score >= 0.5  ? "text-zinc-400"   :
                    "text-zinc-600";
  return (
    <span className={`text-xs ${color}`} title={`importance: ${pct}% (${source})`}>
      ★ {pct}%
    </span>
  );
}

// ── MemoryTable sub-component ─────────────────────────────────────────────────

function MemoryTable({
  memories,
  total,
  loaded,
  agentId,
  loading,
  onDelete,
}: {
  memories: MemoryDto[];
  total: number | undefined;
  loaded: number;
  agentId: string;
  loading: boolean;
  onDelete: (id: string) => void;
}) {
  if (!agentId) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        Enter an Agent ID above to view its memories.
      </div>
    );
  }
  if (loading) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm animate-pulse">
        Loading…
      </div>
    );
  }
  if (memories.length === 0) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        No memories found for <code className="bg-zinc-800 px-1 rounded">{agentId}</code>.
      </div>
    );
  }
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden">
      <div className="px-5 py-3 border-b border-zinc-800">
        <span className="text-sm font-semibold">
          Memories
          <span className="text-zinc-500 font-normal ml-2">
            {loaded} shown / {total ?? "?"} total
          </span>
        </span>
      </div>
      <div className="divide-y divide-zinc-800">
        {memories.map((m) => (
          <div
            key={m.id}
            className="px-5 py-4 hover:bg-zinc-800/40 transition-colors group"
          >
            <div className="flex items-start justify-between gap-4">
              <div className="flex-1 min-w-0">
                <p className="text-sm text-zinc-200 leading-relaxed">{m.content}</p>
                <div className="flex flex-wrap items-center gap-2 mt-2">
                  <span
                    className={`text-xs px-2 py-0.5 rounded-full border ${
                      TYPE_COLORS[m.memory_type] ?? "bg-zinc-700 text-zinc-300 border-zinc-600"
                    }`}
                  >
                    {m.memory_type}
                  </span>
                  <span className={`text-xs ${PROV_COLORS[m.provenance] ?? "text-zinc-500"}`}>
                    {m.provenance}
                  </span>
                  <ImportanceBadge score={m.importance_score} source={m.importance_source} />
                  <span className="text-xs text-zinc-500">
                    conf: {(m.confidence * 100).toFixed(0)}%
                  </span>
                  {m.source_turn !== null && (
                    <span className="text-xs text-zinc-500">turn {m.source_turn}</span>
                  )}
                  {m.session_id && (
                    <span className="text-xs text-zinc-600 font-mono truncate max-w-[160px]">
                      {m.session_id}
                    </span>
                  )}
                  <span className="text-xs text-zinc-600">
                    {new Date(m.created_at).toLocaleString()}
                  </span>
                </div>
              </div>
              <button
                onClick={() => onDelete(m.id)}
                className="opacity-0 group-hover:opacity-100 p-1.5 rounded-lg hover:bg-red-500/20 hover:text-red-400 text-zinc-600 transition-all shrink-0"
                title="Delete memory"
              >
                <Trash2 className="w-4 h-4" />
              </button>
            </div>
          </div>
        ))}
      </div>
    </div>
  );
}
