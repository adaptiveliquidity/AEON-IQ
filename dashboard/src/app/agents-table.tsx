"use client";

import { useState } from "react";
import { Trash2 } from "lucide-react";

interface AgentInfo {
  agent_id: string;
  memory_count: number;
}

export function AgentsTable({
  agents: initial,
  isAdmin,
}: {
  agents: AgentInfo[];
  isAdmin: boolean;
}) {
  const [agents, setAgents] = useState(initial);
  const [deleting, setDeleting] = useState<string | null>(null);

  async function handleDelete(agentId: string) {
    if (!confirm(`Delete agent "${agentId}" and all its memories? This cannot be undone.`)) return;
    setDeleting(agentId);
    try {
      const res = await fetch(`/api/agents/${encodeURIComponent(agentId)}`, {
        method: "DELETE",
      });
      if (res.ok || res.status === 204) {
        setAgents((prev) => prev.filter((a) => a.agent_id !== agentId));
      } else {
        const body = await res.json().catch(() => ({}));
        alert(`Failed to delete agent: ${body.error ?? res.statusText}`);
      }
    } finally {
      setDeleting(null);
    }
  }

  if (agents.length === 0) return null;

  return (
    <table className="w-full text-sm">
      <thead>
        <tr className="text-zinc-500 text-xs uppercase tracking-wide">
          <th className="px-5 py-3 text-left">Agent ID</th>
          <th className="px-5 py-3 text-right">Memories</th>
          {isAdmin && <th className="px-5 py-3 text-right">Actions</th>}
        </tr>
      </thead>
      <tbody className="divide-y divide-zinc-800">
        {agents.map((a) => (
          <tr key={a.agent_id} className="hover:bg-zinc-800/50 transition-colors">
            <td className="px-5 py-3 font-mono text-zinc-300">{a.agent_id}</td>
            <td className="px-5 py-3 text-right text-green-400 font-semibold">
              {a.memory_count}
            </td>
            {isAdmin && (
              <td className="px-5 py-3 text-right">
                <button
                  onClick={() => handleDelete(a.agent_id)}
                  disabled={deleting === a.agent_id}
                  className="inline-flex items-center gap-1 text-xs text-zinc-500 hover:text-red-400 disabled:opacity-40 transition-colors"
                >
                  <Trash2 className="w-3.5 h-3.5" />
                  {deleting === a.agent_id ? "Deleting…" : "Delete"}
                </button>
              </td>
            )}
          </tr>
        ))}
      </tbody>
    </table>
  );
}
