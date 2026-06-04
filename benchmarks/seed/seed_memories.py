#!/usr/bin/env python3
"""Seed deterministic benchmark memories directly into local Postgres."""

from __future__ import annotations

import argparse
import os
from datetime import datetime, timedelta, timezone
from pathlib import Path
from typing import Any

import sys

sys.path.insert(0, str(Path(__file__).resolve().parents[1] / "scripts"))
from common import (  # noqa: E402
    BENCH_AGENT_PREFIX,
    DATABASE_URL,
    default_results_dir,
    hash_embedding,
    load_dataset,
    not_run,
    utc_now,
    vector_literal,
    write_json,
)


def import_psycopg(results_dir: Path):
    try:
        import psycopg
    except Exception as exc:
        return None, not_run(
            "seed",
            f"psycopg is required for deterministic DB seeding: {exc}",
            results_dir,
            "seed_summary.json",
        )
    return psycopg, None


def insert_memory(
    cur: Any,
    agent_id: str,
    content: str,
    memory_type: str = "semantic",
    created_at: datetime | None = None,
    tier: str = "L2",
    importance_score: float = 0.5,
    label: str | None = None,
) -> str:
    created_at = created_at or datetime.now(timezone.utc)
    embedding = vector_literal(hash_embedding(content))
    cur.execute(
        """
        INSERT INTO memories
            (agent_id, session_id, content, memory_type, confidence, embedding,
             created_at, updated_at, source_turn, tier, provenance,
             importance_score, importance_source, status, sensitivity)
        VALUES (%s, %s, %s, %s, %s, %s::vector, %s, %s, %s, %s, %s, %s, %s, 'active', 'unknown')
        RETURNING id
        """,
        (
            agent_id,
            f"{agent_id}-seed",
            content,
            memory_type,
            1.0,
            embedding,
            created_at,
            created_at,
            None,
            tier,
            "user_stated",
            importance_score,
            "user_stated",
        ),
    )
    memory_id = str(cur.fetchone()[0])
    cur.execute(
        """
        INSERT INTO memory_versions
            (memory_id, agent_id, version_number, content, memory_type, confidence,
             provenance, importance_score, importance_source, status, sensitivity,
             source_turn, change_type, change_reason, changed_by, created_at)
        VALUES (%s, %s, 1, %s, %s, %s, 'user_stated', %s, %s, 'active', 'unknown',
                NULL, 'initial', 'benchmark seed', 'benchmark', %s)
        """,
        (
            memory_id,
            agent_id,
            content,
            memory_type,
            1.0,
            importance_score,
            "user_stated",
            created_at,
        ),
    )
    return memory_id


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    parser.add_argument("--include-10000", action="store_true")
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)

    psycopg, skipped = import_psycopg(args.results_dir)
    if skipped:
        return 0

    dataset = load_dataset()
    counts = [100, 1000]
    include_10000 = args.include_10000 or os.environ.get("BENCHMARK_INCLUDE_10000", "").lower() in {
        "1",
        "true",
        "yes",
    }
    if include_10000:
        counts.append(10000)

    summary: dict[str, Any] = {
        "status": "ok",
        "generated_at": utc_now(),
        "agents": {},
        "seed_memory_counts": counts,
        "labels": {},
    }

    with psycopg.connect(DATABASE_URL) as conn:
        with conn.cursor() as cur:
            pattern = f"{BENCH_AGENT_PREFIX}%"
            cur.execute("DELETE FROM memories WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM working_memory WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM memory_retrieval_logs WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM entities WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM memory_graph WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM archival_batches WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM sessions WHERE agent_id LIKE %s", (pattern,))
            cur.execute("DELETE FROM agents WHERE agent_id LIKE %s", (pattern,))

            recall_agent = dataset["agent_id"]
            cur.execute("INSERT INTO agents (agent_id) VALUES (%s) ON CONFLICT DO NOTHING", (recall_agent,))
            for item in dataset["memories"]:
                memory_id = insert_memory(
                    cur,
                    recall_agent,
                    item["content"],
                    importance_score=0.7,
                    label=item["id_label"],
                )
                summary["labels"][item["id_label"]] = memory_id
            summary["agents"][recall_agent] = {"memory_count": len(dataset["memories"])}

            latency_agent = "bench-latency-seeded"
            cur.execute("INSERT INTO agents (agent_id) VALUES (%s) ON CONFLICT DO NOTHING", (latency_agent,))
            for i in range(100):
                insert_memory(
                    cur,
                    latency_agent,
                    f"Benchmark latency memory topic {i}: Nimbus service probe marker {i}.",
                    importance_score=0.5,
                    label="latency_seed",
                )
            summary["agents"][latency_agent] = {"memory_count": 100}

            empty_agent = "bench-latency-empty"
            cur.execute("INSERT INTO agents (agent_id) VALUES (%s) ON CONFLICT DO NOTHING", (empty_agent,))
            summary["agents"][empty_agent] = {"memory_count": 0}

            for count in counts:
                agent_id = f"bench-retrieval-{count}"
                cur.execute("INSERT INTO agents (agent_id) VALUES (%s) ON CONFLICT DO NOTHING", (agent_id,))
                for i in range(count):
                    insert_memory(
                        cur,
                        agent_id,
                        f"Retrieval scale {count} memory {i}: vector probe topic {i % 25} Nimbus benchmark.",
                        importance_score=0.5,
                        label=f"retrieval_{count}",
                    )
                summary["agents"][agent_id] = {"memory_count": count}

            archival_agent = "bench-archival"
            cur.execute("INSERT INTO agents (agent_id) VALUES (%s) ON CONFLICT DO NOTHING", (archival_agent,))
            old = datetime.now(timezone.utc) - timedelta(days=30)
            for i in range(12):
                insert_memory(
                    cur,
                    archival_agent,
                    f"Old archival source {i}: Mira discussed Nimbus Rust service planning and audit trails.",
                    created_at=old + timedelta(minutes=i),
                    importance_score=0.4,
                    label="archival_seed",
                )
            summary["agents"][archival_agent] = {"memory_count": 12}

        conn.commit()

    write_json(args.results_dir / "seed_summary.json", summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
