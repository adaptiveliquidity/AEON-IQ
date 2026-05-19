"""Response model dataclasses for the AEON-IQ SDK."""

from __future__ import annotations

from dataclasses import dataclass, field
from datetime import datetime
from typing import Optional


@dataclass
class Memory:
    id: str
    agent_id: str
    content: str
    memory_type: str
    confidence: float
    provenance: str
    created_at: datetime
    importance_score: float
    importance_source: str
    session_id: Optional[str] = None
    source_turn: Optional[int] = None

    @classmethod
    def from_dict(cls, d: dict) -> "Memory":
        return cls(
            id=d["id"],
            agent_id=d["agent_id"],
            content=d["content"],
            memory_type=d["memory_type"],
            confidence=float(d["confidence"]),
            provenance=d["provenance"],
            created_at=datetime.fromisoformat(d["created_at"].replace("Z", "+00:00")),
            importance_score=float(d["importance_score"]),
            importance_source=d["importance_source"],
            session_id=d.get("session_id"),
            source_turn=d.get("source_turn"),
        )


@dataclass
class MemorySearchResult:
    memories: list[Memory]
    query: str

    @classmethod
    def from_dict(cls, d: dict, query: str = "") -> "MemorySearchResult":
        memories = [Memory.from_dict(m) for m in d.get("memories", [])]
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
            agent_id=d["agent_id"],
            summary=d.get("summary"),
            turn_count=int(d.get("turn_count", 0)),
            updated_at=datetime.fromisoformat(d["updated_at"].replace("Z", "+00:00")),
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
