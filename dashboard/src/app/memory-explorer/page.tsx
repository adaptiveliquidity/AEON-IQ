"use client";

import { useEffect, useState, useCallback } from "react";
import { Search, Trash2, Plus, RefreshCw } from "lucide-react";

interface MemoryDto {
  id: string;
  content: string;
  memory_type: string;
  confidence: number;
  created_at: string;
  session_id: string | null;
  source_turn: number | null;
}

interface MemoryList {
  memories: MemoryDto[];
  total: number;
}

const TYPE_COLORS: Record<string, string> = {
  episodic:   "bg-blue-500/20 text-blue-300",
  semantic:   "bg-green-500/20 text-green-300",
  procedural: "bg-yellow-500/20 text-yellow-300",
};

export default function MemoryExplorerPage() {
  const [agentId, setAgentId]       = useState("");
  const [query, setQuery]           = useState("");
  const [data, setData]             = useState<MemoryList | null>(null);
  const [loading, setLoading]       = useState(false);
  const [adding, setAdding]         = useState(false);
  const [newContent, setNewContent] = useState("");
  const [newType, setNewType]       = useState("semantic");
  const [error, setError]           = useState<string | null>(null);

  const load = useCallback(async () => {
    if (!agentId.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(`/api/agents/${encodeURIComponent(agentId)}/memories`);
      if (!res.ok) throw new Error(await res.text());
      setData(await res.json());
    } catch (e: unknown) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [agentId]);

  useEffect(() => {
    const t = setTimeout(() => { if (agentId) load(); }, 400);
    return () => clearTimeout(t);
  }, [agentId, load]);

  const handleDelete = async (id: string) => {
    if (!confirm("Delete this memory?")) return;
    await fetch(`/api/memories/${id}`, { method: "DELETE" });
    load();
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
      load();
    } finally {
      setLoading(false);
    }
  };

  const filtered = data?.memories.filter((m) =>
    !query || m.content.toLowerCase().includes(query.toLowerCase())
  ) ?? [];

  return (
    <div className="space-y-6">
      <div className="flex items-center justify-between">
        <div>
          <h1 className="text-2xl font-bold">Memory Explorer</h1>
          <p className="text-zinc-400 text-sm mt-1">
            Browse, search, and manage per-agent memories
          </p>
        </div>
        <div className="flex gap-2">
          <button
            onClick={load}
            disabled={loading || !agentId}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 transition-colors"
          >
            <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
            Refresh
          </button>
          <button
            onClick={() => setAdding(!adding)}
            disabled={!agentId}
            className="flex items-center gap-1.5 px-3 py-1.5 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 transition-colors font-medium"
          >
            <Plus className="w-3.5 h-3.5" />
            Add Memory
          </button>
        </div>
      </div>

      {/* Agent ID input */}
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 flex gap-4">
        <div className="flex-1">
          <label className="block text-xs text-zinc-400 mb-1">Agent ID</label>
          <input
            value={agentId}
            onChange={(e) => setAgentId(e.target.value)}
            placeholder="e.g. my-agent-001"
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500"
          />
        </div>
        <div className="flex-1">
          <label className="block text-xs text-zinc-400 mb-1">
            Filter content <span className="text-zinc-600">(client-side)</span>
          </label>
          <div className="relative">
            <Search className="absolute left-3 top-1/2 -translate-y-1/2 w-3.5 h-3.5 text-zinc-500" />
            <input
              value={query}
              onChange={(e) => setQuery(e.target.value)}
              placeholder="Search memories..."
              className="w-full bg-zinc-800 border border-zinc-700 rounded-lg pl-9 pr-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500"
            />
          </div>
        </div>
      </div>

      {/* Add memory form */}
      {adding && (
        <div className="rounded-xl border border-green-600/30 bg-zinc-900 p-4 space-y-3">
          <h3 className="text-sm font-semibold text-green-400">New Memory</h3>
          <textarea
            value={newContent}
            onChange={(e) => setNewContent(e.target.value)}
            rows={3}
            placeholder="Enter memory content..."
            className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-2 text-sm focus:outline-none focus:ring-1 focus:ring-green-500 resize-none"
          />
          <div className="flex items-center gap-3">
            <select
              value={newType}
              onChange={(e) => setNewType(e.target.value)}
              className="bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm focus:outline-none"
            >
              <option value="semantic">semantic</option>
              <option value="episodic">episodic</option>
              <option value="procedural">procedural</option>
            </select>
            <button
              onClick={handleAdd}
              disabled={!newContent.trim()}
              className="px-4 py-1.5 text-sm rounded-lg bg-green-600 hover:bg-green-500 disabled:opacity-40 transition-colors font-medium"
            >
              Save
            </button>
            <button
              onClick={() => setAdding(false)}
              className="px-4 py-1.5 text-sm rounded-lg border border-zinc-700 hover:bg-zinc-800 transition-colors"
            >
              Cancel
            </button>
          </div>
        </div>
      )}

      {/* Error */}
      {error && (
        <div className="rounded-xl border border-red-600/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* Memory table */}
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden">
        <div className="px-5 py-4 border-b border-zinc-800 flex items-center justify-between">
          <h2 className="font-semibold">
            Memories
            {data && (
              <span className="text-zinc-500 font-normal ml-2 text-sm">
                {filtered.length} / {data.total} total
              </span>
            )}
          </h2>
        </div>

        {!agentId ? (
          <div className="px-5 py-12 text-center text-zinc-500 text-sm">
            Enter an Agent ID above to view its memories.
          </div>
        ) : loading ? (
          <div className="px-5 py-12 text-center text-zinc-500 text-sm">
            Loading...
          </div>
        ) : filtered.length === 0 ? (
          <div className="px-5 py-12 text-center text-zinc-500 text-sm">
            No memories found for <code className="bg-zinc-800 px-1 rounded">{agentId}</code>.
          </div>
        ) : (
          <div className="divide-y divide-zinc-800">
            {filtered.map((m) => (
              <div key={m.id} className="px-5 py-4 hover:bg-zinc-800/40 transition-colors group">
                <div className="flex items-start justify-between gap-4">
                  <div className="flex-1 min-w-0">
                    <p className="text-sm text-zinc-200 leading-relaxed">{m.content}</p>
                    <div className="flex items-center flex-wrap gap-2 mt-2">
                      <span
                        className={`text-xs px-2 py-0.5 rounded-full font-medium ${
                          TYPE_COLORS[m.memory_type] ?? "bg-zinc-700 text-zinc-300"
                        }`}
                      >
                        {m.memory_type}
                      </span>
                      <span className="text-xs text-zinc-500">
                        conf: {(m.confidence * 100).toFixed(0)}%
                      </span>
                      {m.source_turn !== null && (
                        <span className="text-xs text-zinc-500">turn {m.source_turn}</span>
                      )}
                      {m.session_id && (
                        <span className="text-xs text-zinc-600 font-mono truncate max-w-[200px]">
                          {m.session_id}
                        </span>
                      )}
                      <span className="text-xs text-zinc-600">
                        {new Date(m.created_at).toLocaleString()}
                      </span>
                    </div>
                  </div>
                  <button
                    onClick={() => handleDelete(m.id)}
                    className="opacity-0 group-hover:opacity-100 p-1.5 rounded-lg hover:bg-red-500/20 hover:text-red-400 text-zinc-600 transition-all"
                    title="Delete memory"
                  >
                    <Trash2 className="w-4 h-4" />
                  </button>
                </div>
              </div>
            ))}
          </div>
        )}
      </div>
    </div>
  );
}
