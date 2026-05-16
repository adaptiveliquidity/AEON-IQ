import { Bot, Brain, Coins, Zap } from "lucide-react";
import { auth } from "@/auth";
import { backendUrl, mgmtHeaders } from "@/lib/backend";

interface Stats {
  agent_count: number;
  memory_count: number;
  tokens_saved_estimate: number;
}

interface AgentInfo {
  agent_id: string;
  memory_count: number;
}

interface AgentList {
  agents: AgentInfo[];
  total: number;
}

async function fetchStats(): Promise<Stats> {
  try {
    const res = await fetch(backendUrl("/api/v1/stats"), {
      cache: "no-store",
      headers: mgmtHeaders(),
    });
    if (!res.ok) throw new Error(res.statusText);
    return res.json();
  } catch {
    return { agent_count: 0, memory_count: 0, tokens_saved_estimate: 0 };
  }
}

async function fetchAgents(): Promise<AgentList> {
  try {
    const res = await fetch(backendUrl("/api/v1/agents"), {
      cache: "no-store",
      headers: mgmtHeaders(),
    });
    if (!res.ok) throw new Error(res.statusText);
    return res.json();
  } catch {
    return { agents: [], total: 0 };
  }
}

function StatCard({
  icon: Icon,
  label,
  value,
  sub,
  color,
}: {
  icon: React.ElementType;
  label: string;
  value: string | number;
  sub?: string;
  color: string;
}) {
  return (
    <div className="rounded-xl border border-zinc-800 bg-zinc-900 p-5">
      <div className="flex items-center justify-between mb-3">
        <span className="text-sm text-zinc-400">{label}</span>
        <div className={`p-2 rounded-lg ${color}`}>
          <Icon className="w-4 h-4" />
        </div>
      </div>
      <p className="text-3xl font-bold">{value}</p>
      {sub && <p className="text-xs text-zinc-500 mt-1">{sub}</p>}
    </div>
  );
}

export default async function OverviewPage() {
  const session = await auth();
  const isAdmin   = session?.user?.isAdmin ?? false;
  const agentId   = session?.user?.agentId ?? "";

  const [stats, agents] = await Promise.all([fetchStats(), fetchAgents()]);

  // Non-admins see only their own agent in the table.
  const visibleAgents: AgentInfo[] = isAdmin
    ? agents.agents
    : agents.agents.filter((a) => a.agent_id === agentId);

  const myAgent = visibleAgents.find((a) => a.agent_id === agentId);
  const costSaved = ((stats.tokens_saved_estimate / 1_000_000) * 2.5).toFixed(4);

  return (
    <div className="space-y-8">
      <div>
        <h1 className="text-2xl font-bold">Overview</h1>
        <p className="text-zinc-400 text-sm mt-1">
          {isAdmin
            ? "Global view of your MemoryOS Kernel instance"
            : `Your agent: ${agentId}`}
        </p>
      </div>

      {/* Stat cards */}
      <div className="grid grid-cols-2 md:grid-cols-4 gap-4">
        <StatCard
          icon={Bot}
          label={isAdmin ? "Active Agents" : "Your Memories"}
          value={isAdmin ? stats.agent_count : (myAgent?.memory_count ?? 0)}
          sub={isAdmin ? "unique agent IDs seen" : `agent: ${agentId}`}
          color="bg-blue-500/10 text-blue-400"
        />
        <StatCard
          icon={Brain}
          label="Memories Stored"
          value={stats.memory_count.toLocaleString()}
          sub={isAdmin ? "across all agents" : "total in kernel"}
          color="bg-green-500/10 text-green-400"
        />
        <StatCard
          icon={Zap}
          label="Tokens Saved (est.)"
          value={stats.tokens_saved_estimate.toLocaleString()}
          sub="vs. naive context stuffing"
          color="bg-yellow-500/10 text-yellow-400"
        />
        <StatCard
          icon={Coins}
          label="Cost Saved (est.)"
          value={`$${costSaved}`}
          sub="at $2.50 / 1M tokens"
          color="bg-purple-500/10 text-purple-400"
        />
      </div>

      {/* Agent table */}
      <div className="rounded-xl border border-zinc-800 bg-zinc-900 overflow-hidden">
        <div className="px-5 py-4 border-b border-zinc-800">
          <h2 className="font-semibold">
            {isAdmin ? `Agents (${agents.total})` : "Your Agent"}
          </h2>
        </div>
        {visibleAgents.length === 0 ? (
          <div className="px-5 py-12 text-center text-zinc-500 text-sm">
            {isAdmin ? (
              <>
                No agents yet. Send a request with{" "}
                <code className="bg-zinc-800 px-1.5 py-0.5 rounded text-xs">x-agent-id</code>{" "}
                to get started.
              </>
            ) : (
              <>
                No memories yet for{" "}
                <code className="bg-zinc-800 px-1.5 py-0.5 rounded text-xs">{agentId}</code>.
                Point your OpenAI client at this proxy with{" "}
                <code className="bg-zinc-800 px-1.5 py-0.5 rounded text-xs">
                  x-agent-id: {agentId}
                </code>
                .
              </>
            )}
          </div>
        ) : (
          <table className="w-full text-sm">
            <thead>
              <tr className="text-zinc-500 text-xs uppercase tracking-wide">
                <th className="px-5 py-3 text-left">Agent ID</th>
                <th className="px-5 py-3 text-right">Memories</th>
              </tr>
            </thead>
            <tbody className="divide-y divide-zinc-800">
              {visibleAgents.map((a) => (
                <tr key={a.agent_id} className="hover:bg-zinc-800/50 transition-colors">
                  <td className="px-5 py-3 font-mono text-zinc-300">{a.agent_id}</td>
                  <td className="px-5 py-3 text-right text-green-400 font-semibold">
                    {a.memory_count}
                  </td>
                </tr>
              ))}
            </tbody>
          </table>
        )}
      </div>
    </div>
  );
}
