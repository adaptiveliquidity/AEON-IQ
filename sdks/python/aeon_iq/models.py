"""Response model dataclasses for the AEON-IQ SDK."""

from __future__ import annotations

from dataclasses import dataclass
from datetime import datetime
from typing import Optional


def _parse_dt(value: Optional[str]) -> Optional[datetime]:
    if not value:
        return None
    return datetime.fromisoformat(value.replace("Z", "+00:00"))


@dataclass
class Memory:
    id: str
    agent_id: str
    content: str
    memory_type: str
    confidence: float
    provenance: str
    created_at: datetime
    updated_at: Optional[datetime] = None
    importance_score: float = 0.5
    importance_source: str = "extractor"
    session_id: Optional[str] = None
    source_turn: Optional[int] = None
    status: Optional[str] = None
    sensitivity: Optional[str] = None

    @classmethod
    def from_dict(cls, d: dict) -> "Memory":
        return cls(
            id=d["id"],
            agent_id=d.get("agent_id", ""),
            content=d["content"],
            memory_type=d["memory_type"],
            confidence=float(d["confidence"]),
            provenance=d.get("provenance", "unknown"),
            created_at=_parse_dt(d["created_at"]) or datetime.now().astimezone(),
            updated_at=_parse_dt(d.get("updated_at")),
            importance_score=float(d["importance_score"]),
            importance_source=d.get("importance_source", "extractor"),
            session_id=d.get("session_id"),
            source_turn=d.get("source_turn"),
            status=d.get("status"),
            sensitivity=d.get("sensitivity"),
        )


@dataclass
class MemorySearchResult:
    memories: list[Memory]
    query: str

    @classmethod
    def from_dict(cls, d: dict, query: str = "") -> "MemorySearchResult":
        rows = d.get("results", d.get("memories", []))
        memories = [Memory.from_dict(m) for m in rows]
        return cls(memories=memories, query=query)


@dataclass
class Session:
    session_id: str
    agent_id: str
    summary: Optional[str]
    turn_count: int
    updated_at: datetime

    @classmethod
    def from_dict(cls, d: dict) -> "Session":
        return cls(
            session_id=d["session_id"],
            agent_id=d.get("agent_id", ""),
            summary=d.get("summary", d.get("summary_preview")),
            turn_count=int(d.get("turn_count", 0)),
            updated_at=_parse_dt(d.get("updated_at")) or datetime.now().astimezone(),
        )


@dataclass
class ArchivalBatch:
    id: str
    agent_id: str
    created_at: datetime
    source_count: int
    l3_count: int
    status: str

    @classmethod
    def from_dict(cls, d: dict) -> "ArchivalBatch":
        return cls(
            id=d["id"],
            agent_id=d["agent_id"],
            created_at=datetime.fromisoformat(d["created_at"].replace("Z", "+00:00")),
            source_count=int(d["source_count"]),
            l3_count=int(d["l3_count"]),
            status=d["status"],
        )


@dataclass
class ImportResult:
    imported: int
    skipped_dedup: int
    errors: int
