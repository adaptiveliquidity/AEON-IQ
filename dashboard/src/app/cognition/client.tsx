"use client";

import { useCallback, useEffect, useMemo, useRef, useState } from "react";
import {
  Activity,
  Brain,
  Clock,
  GitCompareArrows,
  History,
  Lock,
  RefreshCw,
  Rewind,
  Search,
  Sparkles,
} from "lucide-react";

// ── Types ──────────────────────────────────────────────────────────────────────

interface TimeTravelMemory {
  id: string;
  version_number: number;
  content: string;
  memory_type: string;
  confidence: number;
  provenance: string;
  importance_score: number;
  importance_source: string;
  status: string;
  sensitivity: string;
  valid_from: string | null;
  valid_to: string | null;
  source_turn: number | null;
  created_at: string;
  version_created_at: string;
}

interface TimeTravelResponse {
  timestamp: string;
  memories: TimeTravelMemory[];
  total: number;
  offset: number;
  limit: number;
}

interface ArchivedDiff {
  memory_id: string;
  content: string;
  memory_type: string;
  archived_at: string;
}

interface ModifiedMemory {
  memory_id: string;
  before: TimeTravelMemory;
  after: TimeTravelMemory;
}

interface StatusChanged {
  memory_id: string;
  old_status: string;
  new_status: string;
}

interface RetrievalActivity {
  total_retrievals: number;
  unique_memories_recalled: number;
}

interface DiffSummary {
  added: number;
  modified: number;
  archived: number;
  status_changed: number;
  total_retrievals: number;
  unique_memories_recalled: number;
}

interface DiffResponse {
  from: string;
  to: string;
  summary: DiffSummary;
  added: TimeTravelMemory[];
  modified: ModifiedMemory[];
  archived: ArchivedDiff[];
  status_changed: StatusChanged[];
  retrieval_activity: RetrievalActivity;
}

interface RetrievalLog {
  id: string;
  agent_id: string;
  session_id: string | null;
  query_hash: string;
  query_text: string | null;
  candidate_memory_ids: string[];
  injected_memory_ids: string[];
  suppressed_memory_ids: string[];
  scores: Record<string, { cosine_dist?: number; importance_score?: number; confidence?: number }>;
  latency_ms: number | null;
  created_at: string;
}

interface RetrievalLogResponse {
  agent_id: string;
  retrievals: RetrievalLog[];
  total: number;
  limit: number;
  offset: number;
}

interface MemoryVersion {
  id: string;
  memory_id: string;
  version_number: number;
  content: string;
  memory_type: string;
  confidence: number;
  provenance: string;
  importance_score: number;
  importance_source: string;
  status: string;
  sensitivity: string;
  source_turn: number | null;
  change_type: string;
  change_reason: string | null;
  changed_by: string;
  created_at: string;
}

interface VersionsResponse {
  versions: MemoryVersion[];
  total: number;
}

const TABS = ["timeline", "time-travel", "diff", "recall"] as const;
type Tab = (typeof TABS)[number];

const TYPE_COLORS: Record<string, string> = {
  episodic: "bg-blue-500/20 text-blue-300 border-blue-500/30",
  semantic: "bg-green-500/20 text-green-300 border-green-500/30",
  procedural: "bg-yellow-500/20 text-yellow-300 border-yellow-500/30",
  narrative: "bg-purple-500/20 text-purple-300 border-purple-500/30",
};

const STATUS_COLORS: Record<string, string> = {
  active: "text-green-400",
  candidate: "text-yellow-400",
  quarantined: "text-orange-400",
  suppressed: "text-red-400",
};

function rfc3339(d: Date): string {
  return d.toISOString();
}

function relative(iso: string): string {
  const diff = Date.now() - new Date(iso).getTime();
  if (diff < 60_000) return `${Math.round(diff / 1000)}s ago`;
  if (diff < 3_600_000) return `${Math.round(diff / 60_000)}m ago`;
  if (diff < 86_400_000) return `${Math.round(diff / 3_600_000)}h ago`;
  return `${Math.round(diff / 86_400_000)}d ago`;
}

function isoForInput(iso: string): string {
  const d = new Date(iso);
  const pad = (n: number) => `${n}`.padStart(2, "0");
  return (
    `${d.getFullYear()}-${pad(d.getMonth() + 1)}-${pad(d.getDate())}T` +
    `${pad(d.getHours())}:${pad(d.getMinutes())}:${pad(d.getSeconds())}`
  );
}

// ── Component ──────────────────────────────────────────────────────────────────

export default function CognitionClient({
  userAgentId,
  isAdmin,
}: {
  userAgentId: string;
  isAdmin: boolean;
}) {
  const [agentId, setAgentId] = useState(userAgentId);
  const [tab, setTab] = useState<Tab>("timeline");
  const [error, setError] = useState<string | null>(null);
  const [loading, setLoading] = useState(false);

  useEffect(() => {
    if (!isAdmin) setAgentId(userAgentId);
  }, [userAgentId, isAdmin]);

  // ── Timeline + Recall Debugger state ──────────────────────────────────────
  const [retrievals, setRetrievals] = useState<RetrievalLog[]>([]);
  const [retrievalsLoaded, setRetrievalsLoaded] = useState(false);
  const [selectedLogId, setSelectedLogId] = useState<string | null>(null);

  const loadRetrievals = useCallback(async () => {
    if (!agentId.trim()) return;
    setLoading(true);
    setError(null);
    try {
      const res = await fetch(
        `/api/agents/${encodeURIComponent(agentId)}/retrievals?limit=50`
      );
      if (!res.ok) throw new Error(await res.text());
      const data: RetrievalLogResponse = await res.json();
      setRetrievals(data.retrievals ?? []);
      setRetrievalsLoaded(true);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [agentId]);

  // ── Time travel state ──────────────────────────────────────────────────────
  const initialNow = useMemo(() => new Date(), []);
  const [travelTimestamp, setTravelTimestamp] = useState<string>(
    rfc3339(initialNow)
  );
  const [travelData, setTravelData] = useState<TimeTravelResponse | null>(null);
  const debounceRef = useRef<ReturnType<typeof setTimeout> | null>(null);

  const loadTimeTravel = useCallback(
    async (ts: string) => {
      if (!agentId.trim()) return;
      setLoading(true);
      setError(null);
      try {
        const url = `/api/agents/${encodeURIComponent(
          agentId
        )}/memories/at?timestamp=${encodeURIComponent(ts)}&limit=100`;
        const res = await fetch(url);
        if (!res.ok) throw new Error(await res.text());
        const data: TimeTravelResponse = await res.json();
        setTravelData(data);
      } catch (e) {
        setError(e instanceof Error ? e.message : String(e));
      } finally {
        setLoading(false);
      }
    },
    [agentId]
  );

  // 300ms debounce on the time travel timestamp.
  useEffect(() => {
    if (tab !== "time-travel" || !agentId.trim()) return;
    if (debounceRef.current) clearTimeout(debounceRef.current);
    debounceRef.current = setTimeout(() => {
      loadTimeTravel(travelTimestamp);
    }, 300);
    return () => {
      if (debounceRef.current) clearTimeout(debounceRef.current);
    };
  }, [tab, agentId, travelTimestamp, loadTimeTravel]);

  // ── Diff state ─────────────────────────────────────────────────────────────
  const [diffFrom, setDiffFrom] = useState<string>(
    rfc3339(new Date(initialNow.getTime() - 24 * 60 * 60 * 1000))
  );
  const [diffTo, setDiffTo] = useState<string>(rfc3339(initialNow));
  const [diffData, setDiffData] = useState<DiffResponse | null>(null);
  const [diffSection, setDiffSection] = useState<
    "added" | "modified" | "archived" | "status_changed"
  >("added");

  const loadDiff = useCallback(async () => {
    if (!agentId.trim()) return;
    if (!diffFrom || !diffTo) return;
    if (new Date(diffFrom) >= new Date(diffTo)) {
      setError("'from' must be earlier than 'to'");
      return;
    }
    setLoading(true);
    setError(null);
    try {
      const url = `/api/agents/${encodeURIComponent(
        agentId
      )}/memories/diff?from=${encodeURIComponent(diffFrom)}&to=${encodeURIComponent(diffTo)}`;
      const res = await fetch(url);
      if (!res.ok) throw new Error(await res.text());
      const data: DiffResponse = await res.json();
      setDiffData(data);
    } catch (e) {
      setError(e instanceof Error ? e.message : String(e));
    } finally {
      setLoading(false);
    }
  }, [agentId, diffFrom, diffTo]);

  // ── Auto-load when entering tabs ──────────────────────────────────────────
  useEffect(() => {
    if (!agentId.trim()) return;
    if ((tab === "timeline" || tab === "recall") && !retrievalsLoaded) {
      loadRetrievals();
    }
  }, [agentId, tab, retrievalsLoaded, loadRetrievals]);

  // Reset retrievals when agent changes
  useEffect(() => {
    setRetrievalsLoaded(false);
    setRetrievals([]);
    setSelectedLogId(null);
  }, [agentId]);

  // ── Render ─────────────────────────────────────────────────────────────────

  return (
    <div className="space-y-6">
      <div>
        <h1 className="text-2xl font-bold flex items-center gap-2">
          <Sparkles className="w-6 h-6 text-purple-400" />
          Cognition Observability
        </h1>
        <p className="text-zinc-400 text-sm mt-1">
          Retrieval timeline, time-travel through memory state, diff between two
          points in time, and a per-call recall debugger.
        </p>
      </div>

      {/* Agent ID */}
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
              ? "focus:ring-1 focus:ring-purple-500"
              : "cursor-default opacity-70 select-all"
          }`}
        />
      </div>

      {/* Tabs */}
      <div className="flex gap-1 border-b border-zinc-800 pb-0 flex-wrap">
        {(
          [
            { id: "timeline", label: "Retrieval Timeline", icon: Activity },
            { id: "time-travel", label: "Time Travel", icon: Rewind },
            { id: "diff", label: "Diff", icon: GitCompareArrows },
            { id: "recall", label: "Recall Debugger", icon: Brain },
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

      {error && (
        <div className="rounded-xl border border-red-600/30 bg-red-500/10 px-4 py-3 text-sm text-red-400">
          {error}
        </div>
      )}

      {/* ── Timeline tab ─────────────────────────────────────────────────── */}
      {tab === "timeline" && (
        <TimelinePanel
          agentId={agentId}
          retrievals={retrievals}
          loading={loading}
          onRefresh={loadRetrievals}
          onSelect={(id) => {
            setSelectedLogId(id);
            setTab("recall");
          }}
        />
      )}

      {/* ── Time Travel tab ──────────────────────────────────────────────── */}
      {tab === "time-travel" && (
        <TimeTravelPanel
          agentId={agentId}
          timestamp={travelTimestamp}
          onTimestampChange={setTravelTimestamp}
          data={travelData}
          loading={loading}
        />
      )}

      {/* ── Diff tab ─────────────────────────────────────────────────────── */}
      {tab === "diff" && (
        <DiffPanel
          agentId={agentId}
          from={diffFrom}
          to={diffTo}
          onFromChange={setDiffFrom}
          onToChange={setDiffTo}
          onRun={loadDiff}
          data={diffData}
          loading={loading}
          section={diffSection}
          onSectionChange={setDiffSection}
        />
      )}

      {/* ── Recall Debugger tab ──────────────────────────────────────────── */}
      {tab === "recall" && (
        <RecallDebuggerPanel
          agentId={agentId}
          retrievals={retrievals}
          selectedId={selectedLogId}
          onSelect={setSelectedLogId}
          onRefresh={loadRetrievals}
          loading={loading}
        />
      )}
    </div>
  );
}

// ── Timeline panel ─────────────────────────────────────────────────────────────

function TimelinePanel({
  agentId,
  retrievals,
  loading,
  onRefresh,
  onSelect,
}: {
  agentId: string;
  retrievals: RetrievalLog[];
  loading: boolean;
  onRefresh: () => void;
  onSelect: (id: string) => void;
}) {
  if (!agentId) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        Enter an Agent ID above to view its retrieval timeline.
      </div>
    );
  }
  return (
    <div className="space-y-4">
      <div className="flex items-center justify-between">
        <p className="text-sm text-zinc-400">
          Recent retrievals — newest first ({retrievals.length} entries)
        </p>
        <button
          onClick={onRefresh}
          disabled={loading}
          className="flex items-center gap-1.5 px-3 py-2 text-sm rounded-lg border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40 transition-colors"
        >
          <RefreshCw className={`w-3.5 h-3.5 ${loading ? "animate-spin" : ""}`} />
          Refresh
        </button>
      </div>
      {retrievals.length === 0 ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
          No retrievals logged yet for this agent. Send a chat completion through
          the proxy to populate this timeline.
        </div>
      ) : (
        <ol className="relative border-l border-zinc-800 ml-2 space-y-3 pl-6">
          {retrievals.map((r) => {
            const totalCandidates = r.candidate_memory_ids.length;
            const totalInjected = r.injected_memory_ids.length;
            return (
              <li key={r.id} className="relative">
                <span className="absolute -left-[31px] mt-1.5 w-2.5 h-2.5 rounded-full bg-purple-500 border border-purple-300/40" />
                <button
                  onClick={() => onSelect(r.id)}
                  className="w-full text-left rounded-xl border border-zinc-800 bg-zinc-900 px-4 py-3 hover:bg-zinc-800/70 transition-colors group"
                >
                  <div className="flex items-start justify-between gap-3">
                    <div className="flex-1 min-w-0">
                      <p className="text-sm text-zinc-200 truncate">
                        {r.query_text ?? (
                          <span className="text-zinc-500 italic">
                            (query text not stored — set RETRIEVAL_LOG_QUERY_TEXT=true)
                          </span>
                        )}
                      </p>
                      <div className="flex flex-wrap items-center gap-2 mt-2 text-xs">
                        <span className="text-zinc-500">
                          <Clock className="inline w-3 h-3 mr-1" />
                          {relative(r.created_at)}
                        </span>
                        <span className="text-zinc-600">·</span>
                        <span className="text-zinc-400">
                          {totalInjected}/{totalCandidates} injected
                        </span>
                        {r.suppressed_memory_ids.length > 0 && (
                          <>
                            <span className="text-zinc-600">·</span>
                            <span className="text-amber-400">
                              {r.suppressed_memory_ids.length} suppressed
                            </span>
                          </>
                        )}
                        {r.latency_ms !== null && (
                          <>
                            <span className="text-zinc-600">·</span>
                            <span className="text-zinc-500">{r.latency_ms} ms</span>
                          </>
                        )}
                        {r.session_id && (
                          <>
                            <span className="text-zinc-600">·</span>
                            <span className="font-mono text-zinc-600 truncate max-w-[180px]">
                              {r.session_id}
                            </span>
                          </>
                        )}
                      </div>
                    </div>
                    <Search className="w-4 h-4 text-zinc-600 group-hover:text-purple-400 mt-0.5 shrink-0" />
                  </div>
                </button>
              </li>
            );
          })}
        </ol>
      )}
    </div>
  );
}

// ── Time travel panel ─────────────────────────────────────────────────────────

function TimeTravelPanel({
  agentId,
  timestamp,
  onTimestampChange,
  data,
  loading,
}: {
  agentId: string;
  timestamp: string;
  onTimestampChange: (iso: string) => void;
  data: TimeTravelResponse | null;
  loading: boolean;
}) {
  if (!agentId) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        Enter an Agent ID above to scrub through memory state.
      </div>
    );
  }

  const setRelative = (msAgo: number) => {
    onTimestampChange(rfc3339(new Date(Date.now() - msAgo)));
  };

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 space-y-3">
        <div className="flex items-center justify-between">
          <label className="text-xs text-zinc-400">
            View memory state at this timestamp (UTC)
          </label>
          <span className="text-xs text-zinc-600">300ms debounced</span>
        </div>
        <div className="flex flex-wrap items-center gap-2">
          <input
            type="datetime-local"
            step="1"
            value={isoForInput(timestamp)}
            onChange={(e) => {
              const v = e.target.value;
              if (!v) return;
              onTimestampChange(rfc3339(new Date(v)));
            }}
            className="bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-purple-500"
          />
          <span className="text-xs text-zinc-600">
            {new Date(timestamp).toLocaleString()}
          </span>
          <span className="ml-auto flex gap-1">
            {[
              { label: "now", ms: 0 },
              { label: "1h", ms: 3_600_000 },
              { label: "1d", ms: 86_400_000 },
              { label: "7d", ms: 604_800_000 },
              { label: "30d", ms: 2_592_000_000 },
            ].map((p) => (
              <button
                key={p.label}
                onClick={() => setRelative(p.ms)}
                className="px-2 py-1 text-xs rounded-lg border border-zinc-700 hover:bg-zinc-800"
              >
                {p.label === "now" ? "now" : `−${p.label}`}
              </button>
            ))}
          </span>
        </div>
      </div>

      {loading && !data ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm animate-pulse">
          Loading memory state…
        </div>
      ) : !data ? (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
          Pick a timestamp above.
        </div>
      ) : (
        <div className="space-y-3">
          <p className="text-sm text-zinc-400">
            <span className="text-zinc-200 font-semibold">{data.total}</span> live
            memory version{data.total === 1 ? "" : "s"} as of{" "}
            <span className="text-zinc-300">
              {new Date(data.timestamp).toLocaleString()}
            </span>
            {loading && (
              <span className="ml-2 text-purple-400 text-xs animate-pulse">
                refreshing…
              </span>
            )}
          </p>
          {data.memories.length === 0 ? (
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-10 text-center text-zinc-500 text-sm">
              No memories existed at this timestamp.
            </div>
          ) : (
            <div className="rounded-xl border border-zinc-800 bg-zinc-900 divide-y divide-zinc-800">
              {data.memories.map((m) => (
                <MemoryVersionCard key={`${m.id}-${m.version_number}`} memory={m} />
              ))}
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Diff panel ────────────────────────────────────────────────────────────────

function DiffPanel({
  agentId,
  from,
  to,
  onFromChange,
  onToChange,
  onRun,
  data,
  loading,
  section,
  onSectionChange,
}: {
  agentId: string;
  from: string;
  to: string;
  onFromChange: (iso: string) => void;
  onToChange: (iso: string) => void;
  onRun: () => void;
  data: DiffResponse | null;
  loading: boolean;
  section: "added" | "modified" | "archived" | "status_changed";
  onSectionChange: (s: "added" | "modified" | "archived" | "status_changed") => void;
}) {
  if (!agentId) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        Enter an Agent ID above to compute a diff between two points in time.
      </div>
    );
  }

  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 space-y-3">
        <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
          <div>
            <label className="text-xs text-zinc-400 block mb-1">From (UTC)</label>
            <input
              type="datetime-local"
              step="1"
              value={isoForInput(from)}
              onChange={(e) => {
                const v = e.target.value;
                if (!v) return;
                onFromChange(rfc3339(new Date(v)));
              }}
              className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-purple-500"
            />
          </div>
          <div>
            <label className="text-xs text-zinc-400 block mb-1">To (UTC)</label>
            <input
              type="datetime-local"
              step="1"
              value={isoForInput(to)}
              onChange={(e) => {
                const v = e.target.value;
                if (!v) return;
                onToChange(rfc3339(new Date(v)));
              }}
              className="w-full bg-zinc-800 border border-zinc-700 rounded-lg px-3 py-1.5 text-sm focus:outline-none focus:ring-1 focus:ring-purple-500"
            />
          </div>
        </div>
        <div className="flex items-center gap-2 flex-wrap">
          <span className="text-xs text-zinc-500">presets</span>
          {[
            { label: "last hour", from: 3_600_000 },
            { label: "last 24h", from: 86_400_000 },
            { label: "last 7d", from: 604_800_000 },
            { label: "last 30d", from: 2_592_000_000 },
          ].map((p) => (
            <button
              key={p.label}
              onClick={() => {
                const now = new Date();
                onToChange(rfc3339(now));
                onFromChange(rfc3339(new Date(now.getTime() - p.from)));
              }}
              className="px-2 py-1 text-xs rounded-lg border border-zinc-700 hover:bg-zinc-800"
            >
              {p.label}
            </button>
          ))}
          <button
            onClick={onRun}
            disabled={loading}
            className="ml-auto flex items-center gap-1.5 px-4 py-1.5 text-sm rounded-lg bg-purple-600 hover:bg-purple-500 disabled:opacity-40 font-medium"
          >
            <GitCompareArrows
              className={`w-3.5 h-3.5 ${loading ? "animate-pulse" : ""}`}
            />
            Compute diff
          </button>
        </div>
      </div>

      {data && (
        <>
          <div className="grid grid-cols-2 md:grid-cols-6 gap-3">
            <SummaryStat
              label="Added"
              value={data.summary.added}
              color="bg-green-500/10 text-green-400"
            />
            <SummaryStat
              label="Modified"
              value={data.summary.modified}
              color="bg-blue-500/10 text-blue-400"
            />
            <SummaryStat
              label="Archived"
              value={data.summary.archived}
              color="bg-amber-500/10 text-amber-400"
            />
            <SummaryStat
              label="Status changed"
              value={data.summary.status_changed}
              color="bg-purple-500/10 text-purple-400"
            />
            <SummaryStat
              label="Retrievals"
              value={data.summary.total_retrievals}
              color="bg-zinc-800 text-zinc-300"
            />
            <SummaryStat
              label="Recalled"
              value={data.summary.unique_memories_recalled}
              sub="unique memories"
              color="bg-zinc-800 text-zinc-300"
            />
          </div>

          <div className="flex gap-1">
            {(["added", "modified", "archived", "status_changed"] as const).map((s) => {
              const count = data.summary[s];
              const label =
                s === "status_changed"
                  ? "Status changed"
                  : s.charAt(0).toUpperCase() + s.slice(1);
              return (
                <button
                  key={s}
                  onClick={() => onSectionChange(s)}
                  className={`px-3 py-1.5 text-sm rounded-lg transition-colors ${
                    section === s
                      ? "bg-zinc-800 text-zinc-100"
                      : "text-zinc-500 hover:text-zinc-300"
                  }`}
                >
                  {label}{" "}
                  <span className="text-xs text-zinc-600 ml-1">({count})</span>
                </button>
              );
            })}
          </div>

          <DiffSectionView data={data} section={section} />
        </>
      )}

      {!data && !loading && (
        <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
          Pick a time range and click <span className="text-zinc-300">Compute diff</span>{" "}
          to inspect what changed.
        </div>
      )}
    </div>
  );
}

function SummaryStat({
  label,
  value,
  sub,
  color,
}: {
  label: string;
  value: number;
  sub?: string;
  color: string;
}) {
  return (
    <div className={`rounded-xl border border-zinc-800 ${color} px-3 py-2`}>
      <p className="text-xs opacity-80">{label}</p>
      <p className="text-2xl font-bold">{value}</p>
      {sub && <p className="text-[11px] opacity-60">{sub}</p>}
    </div>
  );
}

function DiffSectionView({
  data,
  section,
}: {
  data: DiffResponse;
  section: "added" | "modified" | "archived" | "status_changed";
}) {
  if (section === "added") {
    if (data.added.length === 0) {
      return <EmptyDiff label="No memories added in this window." />;
    }
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 divide-y divide-zinc-800">
        {data.added.map((m) => (
          <MemoryVersionCard key={`${m.id}-added`} memory={m} highlight="added" />
        ))}
      </div>
    );
  }
  if (section === "modified") {
    if (data.modified.length === 0) {
      return <EmptyDiff label="No memories modified in this window." />;
    }
    return (
      <div className="space-y-3">
        {data.modified.map((m) => (
          <div
            key={m.memory_id}
            className="rounded-xl border border-zinc-800 bg-zinc-900 p-4"
          >
            <p className="text-xs text-zinc-500 font-mono mb-2 truncate">
              {m.memory_id}
            </p>
            <div className="grid grid-cols-1 md:grid-cols-2 gap-3">
              <DiffSide label="before" memory={m.before} side="before" />
              <DiffSide label="after" memory={m.after} side="after" />
            </div>
          </div>
        ))}
      </div>
    );
  }
  if (section === "archived") {
    if (data.archived.length === 0) {
      return <EmptyDiff label="No memories archived in this window." />;
    }
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 divide-y divide-zinc-800">
        {data.archived.map((m) => (
          <div key={m.memory_id} className="px-5 py-4">
            <p className="text-sm text-zinc-300">{m.content}</p>
            <div className="flex flex-wrap items-center gap-2 mt-2 text-xs">
              <TypePill type={m.memory_type} />
              <span className="text-amber-400">
                archived {new Date(m.archived_at).toLocaleString()}
              </span>
              <span className="font-mono text-zinc-600 truncate max-w-[200px]">
                {m.memory_id}
              </span>
            </div>
          </div>
        ))}
      </div>
    );
  }
  if (data.status_changed.length === 0) {
    return <EmptyDiff label="No status transitions in this window." />;
  }
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900 divide-y divide-zinc-800">
      {data.status_changed.map((s) => (
        <div key={s.memory_id} className="px-5 py-3 flex items-center gap-3 text-sm">
          <span className="font-mono text-xs text-zinc-500 truncate max-w-[280px]">
            {s.memory_id}
          </span>
          <span
            className={`px-2 py-0.5 rounded-full text-xs ${
              STATUS_COLORS[s.old_status] ?? "text-zinc-400"
            }`}
          >
            {s.old_status}
          </span>
          <span className="text-zinc-600">→</span>
          <span
            className={`px-2 py-0.5 rounded-full text-xs ${
              STATUS_COLORS[s.new_status] ?? "text-zinc-400"
            }`}
          >
            {s.new_status}
          </span>
        </div>
      ))}
    </div>
  );
}

function EmptyDiff({ label }: { label: string }) {
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-10 text-center text-zinc-500 text-sm">
      {label}
    </div>
  );
}

function DiffSide({
  label,
  memory,
  side,
}: {
  label: string;
  memory: TimeTravelMemory;
  side: "before" | "after";
}) {
  const border =
    side === "before"
      ? "border-rose-500/30 bg-rose-500/5"
      : "border-emerald-500/30 bg-emerald-500/5";
  return (
    <div className={`rounded-lg border ${border} px-3 py-2`}>
      <p className="text-[10px] uppercase tracking-wide text-zinc-500 mb-1">
        {label} · v{memory.version_number}
      </p>
      <p className="text-sm text-zinc-200 leading-relaxed">{memory.content}</p>
      <div className="flex flex-wrap items-center gap-2 mt-2 text-xs">
        <TypePill type={memory.memory_type} />
        <span className={STATUS_COLORS[memory.status] ?? "text-zinc-400"}>
          {memory.status}
        </span>
        <span className="text-zinc-500">
          conf {(memory.confidence * 100).toFixed(0)}%
        </span>
      </div>
    </div>
  );
}

// ── Recall debugger ────────────────────────────────────────────────────────────

function RecallDebuggerPanel({
  agentId,
  retrievals,
  selectedId,
  onSelect,
  onRefresh,
  loading,
}: {
  agentId: string;
  retrievals: RetrievalLog[];
  selectedId: string | null;
  onSelect: (id: string | null) => void;
  onRefresh: () => void;
  loading: boolean;
}) {
  const log = useMemo(
    () => retrievals.find((r) => r.id === selectedId) ?? retrievals[0] ?? null,
    [retrievals, selectedId]
  );

  if (!agentId) {
    return (
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
        Enter an Agent ID above to debug retrievals.
      </div>
    );
  }

  return (
    <div className="grid grid-cols-1 md:grid-cols-3 gap-4">
      <div className="md:col-span-1 space-y-2">
        <div className="flex items-center justify-between">
          <p className="text-sm text-zinc-400">Recent retrievals</p>
          <button
            onClick={onRefresh}
            disabled={loading}
            className="flex items-center gap-1 px-2 py-1 text-xs rounded-lg border border-zinc-700 hover:bg-zinc-800 disabled:opacity-40"
          >
            <RefreshCw className={`w-3 h-3 ${loading ? "animate-spin" : ""}`} />
          </button>
        </div>
        {retrievals.length === 0 ? (
          <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-3 py-6 text-center text-zinc-500 text-xs">
            No retrievals yet.
          </div>
        ) : (
          <div className="space-y-1 max-h-[600px] overflow-y-auto pr-1">
            {retrievals.map((r) => {
              const active = log?.id === r.id;
              return (
                <button
                  key={r.id}
                  onClick={() => onSelect(r.id)}
                  className={`w-full text-left rounded-lg border px-3 py-2 transition-colors ${
                    active
                      ? "bg-purple-500/10 border-purple-500/40 text-zinc-100"
                      : "border-zinc-800 bg-zinc-900 hover:bg-zinc-800/60 text-zinc-400"
                  }`}
                >
                  <p className="text-xs truncate">
                    {r.query_text ?? (
                      <span className="italic text-zinc-600">(hash only)</span>
                    )}
                  </p>
                  <p className="text-[11px] text-zinc-600 mt-1">
                    {relative(r.created_at)} · {r.injected_memory_ids.length}/
                    {r.candidate_memory_ids.length} injected
                  </p>
                </button>
              );
            })}
          </div>
        )}
      </div>

      <div className="md:col-span-2">
        {!log ? (
          <div className="rounded-xl border border-zinc-800 bg-zinc-900 px-5 py-12 text-center text-zinc-500 text-sm">
            Select a retrieval on the left to inspect.
          </div>
        ) : (
          <RetrievalDetail log={log} />
        )}
      </div>
    </div>
  );
}

function RetrievalDetail({ log }: { log: RetrievalLog }) {
  return (
    <div className="space-y-4">
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-4 space-y-2">
        <p className="text-sm text-zinc-200">
          {log.query_text ?? (
            <span className="italic text-zinc-500">
              (raw query not stored — set RETRIEVAL_LOG_QUERY_TEXT=true to capture)
            </span>
          )}
        </p>
        <div className="flex flex-wrap gap-2 text-xs text-zinc-500">
          <span>
            <Clock className="inline w-3 h-3 mr-1" />
            {new Date(log.created_at).toLocaleString()}
          </span>
          <span>·</span>
          <span>{log.candidate_memory_ids.length} candidates</span>
          <span>·</span>
          <span className="text-green-400">
            {log.injected_memory_ids.length} injected
          </span>
          {log.suppressed_memory_ids.length > 0 && (
            <>
              <span>·</span>
              <span className="text-amber-400">
                {log.suppressed_memory_ids.length} suppressed
              </span>
            </>
          )}
          {log.latency_ms !== null && (
            <>
              <span>·</span>
              <span>{log.latency_ms} ms</span>
            </>
          )}
        </div>
        <p className="text-[11px] text-zinc-600 font-mono break-all">
          query_hash {log.query_hash}
        </p>
      </div>

      <BucketList
        title={`Injected (${log.injected_memory_ids.length})`}
        accent="text-green-400"
        ids={log.injected_memory_ids}
        scores={log.scores}
      />
      <BucketList
        title={`Suppressed (${log.suppressed_memory_ids.length})`}
        accent="text-amber-400"
        ids={log.suppressed_memory_ids}
        scores={log.scores}
      />
      <BucketList
        title={`All Candidates (${log.candidate_memory_ids.length})`}
        accent="text-zinc-300"
        ids={log.candidate_memory_ids}
        scores={log.scores}
        collapsed
      />
    </div>
  );
}

function BucketList({
  title,
  accent,
  ids,
  scores,
  collapsed = false,
}: {
  title: string;
  accent: string;
  ids: string[];
  scores: RetrievalLog["scores"];
  collapsed?: boolean;
}) {
  const [open, setOpen] = useState(!collapsed);
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900">
      <button
        onClick={() => setOpen(!open)}
        className="w-full px-4 py-3 flex items-center justify-between text-sm"
      >
        <span className={`font-semibold ${accent}`}>{title}</span>
        <span className="text-zinc-600 text-xs">{open ? "hide" : "show"}</span>
      </button>
      {open && ids.length > 0 && (
        <div className="border-t border-zinc-800 divide-y divide-zinc-800">
          {ids.map((id) => {
            const score = scores[id] ?? {};
            return (
              <CandidateRow
                key={id}
                memoryId={id}
                cosineDist={score.cosine_dist}
                importance={score.importance_score}
                confidence={score.confidence}
              />
            );
          })}
        </div>
      )}
      {open && ids.length === 0 && (
        <div className="border-t border-zinc-800 px-4 py-4 text-xs text-zinc-500 text-center">
          (none)
        </div>
      )}
    </div>
  );
}

function CandidateRow({
  memoryId,
  cosineDist,
  importance,
  confidence,
}: {
  memoryId: string;
  cosineDist?: number;
  importance?: number;
  confidence?: number;
}) {
  const [expanded, setExpanded] = useState(false);
  const [versions, setVersions] = useState<MemoryVersion[] | null>(null);
  const [versionLoading, setVersionLoading] = useState(false);

  const onExpand = async () => {
    if (expanded) {
      setExpanded(false);
      return;
    }
    setExpanded(true);
    if (versions !== null) return;
    setVersionLoading(true);
    try {
      const res = await fetch(`/api/memories/${memoryId}/versions`);
      if (!res.ok) throw new Error(await res.text());
      const data: VersionsResponse = await res.json();
      setVersions(data.versions ?? []);
    } catch {
      setVersions([]);
    } finally {
      setVersionLoading(false);
    }
  };

  const latest = versions && versions.length > 0 ? versions[0] : null;

  return (
    <div className="px-4 py-3 hover:bg-zinc-800/40 transition-colors">
      <button
        onClick={onExpand}
        className="w-full text-left flex items-start justify-between gap-3"
      >
        <div className="flex-1 min-w-0">
          <p className="text-xs font-mono text-zinc-500 truncate">{memoryId}</p>
          <div className="flex items-center gap-3 mt-1 text-xs text-zinc-500 flex-wrap">
            {cosineDist !== undefined && (
              <span>
                dist{" "}
                <span className="text-zinc-200">{cosineDist.toFixed(4)}</span>
              </span>
            )}
            {importance !== undefined && (
              <span>
                imp{" "}
                <span className="text-zinc-200">
                  {(importance * 100).toFixed(0)}%
                </span>
              </span>
            )}
            {confidence !== undefined && (
              <span>
                conf{" "}
                <span className="text-zinc-200">
                  {(confidence * 100).toFixed(0)}%
                </span>
              </span>
            )}
          </div>
        </div>
        <History
          className={`w-3.5 h-3.5 mt-0.5 shrink-0 transition-colors ${
            expanded ? "text-purple-400" : "text-zinc-600"
          }`}
        />
      </button>
      {expanded && (
        <div className="mt-2 rounded-lg border border-zinc-800 bg-zinc-950/40 px-3 py-2">
          {versionLoading ? (
            <p className="text-xs text-zinc-500 animate-pulse">
              Loading version history…
            </p>
          ) : !versions || versions.length === 0 ? (
            <p className="text-xs text-zinc-500">No version history available.</p>
          ) : (
            <div className="space-y-2">
              {latest && (
                <div className="rounded-md bg-zinc-900 px-3 py-2">
                  <p className="text-sm text-zinc-200 leading-relaxed">
                    {latest.content}
                  </p>
                  <div className="flex flex-wrap items-center gap-2 mt-1.5 text-xs text-zinc-500">
                    <TypePill type={latest.memory_type} />
                    <span
                      className={STATUS_COLORS[latest.status] ?? "text-zinc-400"}
                    >
                      {latest.status}
                    </span>
                    <span>v{latest.version_number}</span>
                    <span>conf {(latest.confidence * 100).toFixed(0)}%</span>
                  </div>
                </div>
              )}
              <details className="text-xs">
                <summary className="cursor-pointer text-zinc-500 hover:text-zinc-300">
                  Full history ({versions.length})
                </summary>
                <ul className="mt-2 space-y-1.5 pl-4 border-l border-zinc-800">
                  {versions.map((v) => (
                    <li key={v.id}>
                      <p className="text-zinc-300">
                        v{v.version_number}{" "}
                        <span className="text-zinc-600">
                          ({v.change_type}) ·{" "}
                          {new Date(v.created_at).toLocaleString()}
                        </span>
                      </p>
                      <p className="text-zinc-500 truncate">{v.content}</p>
                    </li>
                  ))}
                </ul>
              </details>
            </div>
          )}
        </div>
      )}
    </div>
  );
}

// ── Shared sub-components ──────────────────────────────────────────────────────

function TypePill({ type }: { type: string }) {
  const cls =
    TYPE_COLORS[type] ?? "bg-zinc-700 text-zinc-300 border-zinc-600";
  return (
    <span className={`text-xs px-2 py-0.5 rounded-full border ${cls}`}>
      {type}
    </span>
  );
}

function MemoryVersionCard({
  memory,
  highlight,
}: {
  memory: TimeTravelMemory;
  highlight?: "added";
}) {
  return (
    <div
      className={`px-5 py-4 ${
        highlight === "added" ? "border-l-2 border-l-green-500/60" : ""
      }`}
    >
      <p className="text-sm text-zinc-200 leading-relaxed">{memory.content}</p>
      <div className="flex flex-wrap items-center gap-2 mt-2 text-xs">
        <TypePill type={memory.memory_type} />
        <span className={STATUS_COLORS[memory.status] ?? "text-zinc-400"}>
          {memory.status}
        </span>
        <span className="text-zinc-500">
          v{memory.version_number} · conf {(memory.confidence * 100).toFixed(0)}%
        </span>
        <span className="text-zinc-500">
          imp {(memory.importance_score * 100).toFixed(0)}%
        </span>
        {memory.source_turn !== null && (
          <span className="text-zinc-500">turn {memory.source_turn}</span>
        )}
        <span className="text-zinc-600 font-mono truncate max-w-[200px]">
          {memory.id}
        </span>
        <span className="text-zinc-600">
          created {new Date(memory.created_at).toLocaleString()}
        </span>
      </div>
    </div>
  );
}
