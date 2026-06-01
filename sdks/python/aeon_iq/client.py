"""MemoryClient — typed wrapper around the AEON-IQ management API."""

from __future__ import annotations

import json
import os
from pathlib import Path
from typing import Optional, Union
from urllib.parse import quote

import httpx

from .models import (
    ArchivalBatch,
    ImportResult,
    Memory,
    MemorySearchResult,
    Session,
)


class MemoryClient:
    """Typed client for the AEON-IQ MemoryOS management API.

    Example::

        from aeon_iq import MemoryClient

        client = MemoryClient(url="http://localhost:8080", api_key="...")
        memories = client.search(agent_id="my-bot", query="user preferences")
        client.create(agent_id="my-bot", content="User prefers dark mode", importance=0.8)
        client.export_agent("my-bot", path="backup.ndjson")
    """

    def __init__(
        self,
        url: Optional[str] = None,
        api_key: Optional[str] = None,
        timeout: float = 30.0,
    ) -> None:
        base = (url or os.environ.get("MEMORYOS_URL", "http://localhost:8080")).rstrip("/")
        key = api_key or os.environ.get("MEMORYOS_API_KEY", "")

        headers: dict[str, str] = {}
        if key:
            headers["X-Management-Key"] = key

        self._client = httpx.Client(
            base_url=base,
            headers=headers,
            timeout=timeout,
        )

    # ── Memories ──────────────────────────────────────────────────────────────

    def list_memories(
        self,
        agent_id: str,
        limit: int = 50,
        offset: int = 0,
    ) -> list[Memory]:
        """Return paginated live memories for an agent."""
        resp = self._get(
            f"/api/v1/agents/{quote(agent_id)}/memories",
            params={"limit": limit, "offset": offset},
        )
        return [Memory.from_dict(m) for m in resp.get("memories", [])]

    def create(
        self,
        agent_id: str,
        content: str,
        memory_type: str = "semantic",
        importance: float = 0.8,
        confidence: float = 0.95,
        provenance: str = "user_stated",
    ) -> dict:
        """Manually create a memory for an agent."""
        body = {
            "content": content,
            "memory_type": memory_type,
            "importance_score": importance,
            "importance_source": "user_stated",
            "confidence": confidence,
            "provenance": provenance,
        }
        return self._post(f"/api/v1/agents/{quote(agent_id)}/memories", body)

    def update(self, memory_id: str, content: str) -> dict:
        """Update memory content by ID."""
        return self._patch(f"/api/v1/memories/{quote(memory_id)}", {"content": content})

    def delete(self, memory_id: str) -> None:
        """Hard-delete a memory by ID."""
        self._delete(f"/api/v1/memories/{quote(memory_id)}")

    def search(
        self,
        agent_id: str,
        query: str,
        limit: int = 5,
    ) -> MemorySearchResult:
        """Semantic search over an agent's memories."""
        body = {"query": query, "agent_id": agent_id, "limit": limit}
        resp = self._post("/api/v1/memories/search", body)
        return MemorySearchResult.from_dict(resp, query=query)

    def restore_memory(self, memory_id: str) -> dict:
        """Restore a tombstoned memory."""
        return self._post(f"/api/v1/memories/{quote(memory_id)}/restore", {})

    # ── Sessions ──────────────────────────────────────────────────────────────

    def list_sessions(self, agent_id: str) -> list[Session]:
        """List all working-memory sessions for an agent."""
        resp = self._get(f"/api/v1/agents/{quote(agent_id)}/sessions")
        return [Session.from_dict(s) for s in resp.get("sessions", [])]

    def get_session(self, agent_id: str, session_id: str) -> Session:
        """Get the full L1 summary for a session."""
        resp = self._get(
            f"/api/v1/agents/{quote(agent_id)}/sessions/{quote(session_id)}"
        )
        return Session.from_dict(resp)

    def delete_session(self, agent_id: str, session_id: str) -> None:
        """Clear working memory for a session."""
        self._delete(
            f"/api/v1/agents/{quote(agent_id)}/sessions/{quote(session_id)}"
        )

    # ── Archival ──────────────────────────────────────────────────────────────

    def list_archival_batches(self, agent_id: str) -> list[ArchivalBatch]:
        """List all L2→L3 compaction batches for an agent."""
        resp = self._get(f"/api/v1/agents/{quote(agent_id)}/archival/batches")
        return [ArchivalBatch.from_dict(b) for b in resp.get("batches", [])]

    def restore_batch(self, batch_id: str) -> dict:
        """Restore an entire archival batch (un-tombstone L2, re-tombstone L3)."""
        return self._post(f"/api/v1/archival/batches/{quote(batch_id)}/restore", {})

    def trigger_archival(self, agent_id: str) -> dict:
        """Trigger one archival run for an agent."""
        return self._post(f"/api/v1/agents/{quote(agent_id)}/archival/trigger", {})

    def list_archived_memories(
        self, agent_id: str, limit: int = 50, offset: int = 0
    ) -> list[Memory]:
        """List archived memories for an agent."""
        resp = self._get(
            f"/api/v1/agents/{quote(agent_id)}/memories/archived",
            params={"limit": limit, "offset": offset},
        )
        return [Memory.from_dict(m) for m in resp.get("memories", [])]

    def bulk_operation(
        self,
        agent_id: str,
        action: str,
        *,
        session_id: Optional[str] = None,
        memory_type: Optional[str] = None,
        older_than: Optional[str] = None,
        importance_below: Optional[float] = None,
    ) -> dict:
        """Run bulk archive/delete by filter."""
        body = {
            "action": action,
            "filter": {
                "session_id": session_id,
                "memory_type": memory_type,
                "older_than": older_than,
                "importance_below": importance_below,
            },
        }
        return self._post(f"/api/v1/agents/{quote(agent_id)}/memories/bulk", body)

    # ── Export / Import ───────────────────────────────────────────────────────

    def export_agent(
        self,
        agent_id: str,
        path: Optional[Union[str, Path]] = None,
    ) -> str:
        """Export all memories as NDJSON.

        If *path* is given, writes to that file and returns the path string.
        Otherwise returns the raw NDJSON string.
        """
        resp = self._client.get(f"/api/v1/agents/{quote(agent_id)}/export")
        _raise_for_status(resp)
        ndjson = resp.text

        if path is not None:
            Path(path).write_text(ndjson, encoding="utf-8")
            return str(path)

        return ndjson

    def import_agent(
        self,
        agent_id: str,
        path: Optional[Union[str, Path]] = None,
        ndjson: Optional[str] = None,
    ) -> ImportResult:
        """Import memories from NDJSON (file path or raw string)."""
        if path is not None:
            data = Path(path).read_text(encoding="utf-8")
        elif ndjson is not None:
            data = ndjson
        else:
            raise ValueError("Provide either 'path' or 'ndjson'.")

        resp = self._client.post(
            f"/api/v1/agents/{quote(agent_id)}/import",
            content=data.encode("utf-8"),
            headers={"Content-Type": "application/x-ndjson"},
        )
        _raise_for_status(resp)
        body = resp.json()
        return ImportResult(
            imported=body.get("imported", 0),
            skipped_dedup=body.get("skipped_dedup", 0),
            errors=body.get("errors", 0),
        )

    # ── Stats ─────────────────────────────────────────────────────────────────

    def stats(self) -> dict:
        """Return global memory counts and agent stats."""
        return self._get("/api/v1/stats")

    def list_agents(self) -> dict:
        """List all agents."""
        return self._get("/api/v1/agents")

    def delete_agent(self, agent_id: str) -> None:
        """Delete an agent and all associated data."""
        self._delete(f"/api/v1/agents/{quote(agent_id)}")

    def list_conflicts(self, agent_id: str, include_resolved: bool = False) -> dict:
        """List conflicts for an agent."""
        return self._get(
            f"/api/v1/agents/{quote(agent_id)}/conflicts",
            params={"include_resolved": str(include_resolved).lower()},
        )

    def resolve_conflict(self, conflict_id: str, resolution: str) -> dict:
        """Resolve a conflict using keep_a|keep_b|keep_both|dismissed."""
        return self._post(
            f"/api/v1/conflicts/{quote(conflict_id)}/resolve",
            {"resolution": resolution},
        )

    def list_retrievals(
        self,
        agent_id: str,
        limit: int = 50,
        offset: int = 0,
        session_id: Optional[str] = None,
    ) -> dict:
        """List retrieval logs for an agent."""
        params: dict[str, object] = {"limit": limit, "offset": offset}
        if session_id is not None:
            params["session_id"] = session_id
        return self._get(f"/api/v1/agents/{quote(agent_id)}/retrievals", params=params)

    def list_memory_versions(self, memory_id: str) -> dict:
        """List full version history for one memory."""
        return self._get(f"/api/v1/memories/{quote(memory_id)}/versions")

    def patch_memory_status(
        self, memory_id: str, status: str, reason: Optional[str] = None
    ) -> dict:
        """Update memory status."""
        return self._patch(
            f"/api/v1/memories/{quote(memory_id)}/status",
            {"status": status, "reason": reason},
        )

    def patch_memory_sensitivity(self, memory_id: str, sensitivity: str) -> dict:
        """Update memory sensitivity."""
        return self._patch(
            f"/api/v1/memories/{quote(memory_id)}/sensitivity",
            {"sensitivity": sensitivity},
        )

    def post_feedback(self, agent_id: str, memory_id: str, feedback: float) -> dict:
        """Submit retrieval quality feedback."""
        return self._post(
            "/api/v1/feedback",
            {"agent_id": agent_id, "memory_id": memory_id, "feedback": feedback},
        )

    # ── Private helpers ───────────────────────────────────────────────────────

    def _get(self, path: str, params: Optional[dict] = None) -> dict:
        resp = self._client.get(path, params=params)
        _raise_for_status(resp)
        return resp.json()

    def _post(self, path: str, body: dict) -> dict:
        resp = self._client.post(path, json=body)
        _raise_for_status(resp)
        # Some endpoints return 204 No Content
        if resp.status_code == 204 or not resp.content:
            return {}
        return resp.json()

    def _delete(self, path: str) -> None:
        resp = self._client.delete(path)
        _raise_for_status(resp)

    def _patch(self, path: str, body: dict) -> dict:
        resp = self._client.patch(path, json=body)
        _raise_for_status(resp)
        if resp.status_code == 204 or not resp.content:
            return {}
        return resp.json()

    def __enter__(self) -> "MemoryClient":
        return self

    def __exit__(self, *_: object) -> None:
        self._client.close()

    def close(self) -> None:
        self._client.close()


def _raise_for_status(resp: httpx.Response) -> None:
    if resp.is_error:
        try:
            detail = resp.json()
        except Exception:
            detail = resp.text
        raise httpx.HTTPStatusError(
            f"MemoryOS API error {resp.status_code}: {detail}",
            request=resp.request,
            response=resp,
        )
