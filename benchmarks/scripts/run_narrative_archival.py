#!/usr/bin/env python3
"""Verify narrative archival correctness with deterministic seeded L2 memories."""

from __future__ import annotations

import argparse
from pathlib import Path
from typing import Any

from common import AEON_BASE_URL, DATABASE_URL, default_results_dir, not_run, request_json, write_json


def import_psycopg(results_dir: Path):
    try:
        import psycopg
    except Exception as exc:
        return None, not_run(
            "narrative_archival",
            f"psycopg is required to verify archival DB shape: {exc}",
            results_dir,
            "narrative_archival.json",
        )
    return psycopg, None


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)
    agent_id = "bench-archival"

    psycopg, skipped = import_psycopg(args.results_dir)
    if skipped:
        return 0

    status, trigger, _ = request_json(
        "POST",
        f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/archival/trigger",
        {},
        timeout=60,
    )

    checks: dict[str, bool] = {
        "trigger_http_ok": 200 <= status < 300,
        "trigger_not_skipped": trigger.get("status") != "skipped",
        "narrative_count_is_one": trigger.get("narrative_count") == 1,
    }
    details: dict[str, Any] = {"trigger_response": trigger}
    batch_id = trigger.get("batch_id")

    if batch_id:
        with psycopg.connect(DATABASE_URL) as conn:
            with conn.cursor() as cur:
                cur.execute(
                    """
                    SELECT id, content, tier, memory_type, archival_batch_id, archived_at
                    FROM memories
                    WHERE agent_id = %s AND archival_batch_id = %s
                    ORDER BY tier, memory_type, created_at
                    """,
                    (agent_id, batch_id),
                )
                rows = cur.fetchall()
                memories = [
                    {
                        "id": str(r[0]),
                        "content": r[1],
                        "tier": r[2],
                        "memory_type": r[3],
                        "archival_batch_id": str(r[4]) if r[4] else None,
                        "archived_at": r[5].isoformat() if r[5] else None,
                    }
                    for r in rows
                ]
                l3_narratives = [m for m in memories if m["tier"] == "L3" and m["memory_type"] == "narrative"]
                l2_sources = [m for m in memories if m["tier"] == "L2"]
                narrative_id = l3_narratives[0]["id"] if l3_narratives else None
                version_count = 0
                if narrative_id:
                    cur.execute("SELECT COUNT(*) FROM memory_versions WHERE memory_id = %s", (narrative_id,))
                    version_count = int(cur.fetchone()[0])
                cur.execute(
                    "SELECT source_count, l3_count, status FROM archival_batches WHERE id = %s",
                    (batch_id,),
                )
                batch_row = cur.fetchone()

        checks.update(
            {
                "has_l3_narrative_memory": bool(l3_narratives),
                "narrative_has_version_entry": version_count >= 1,
                "same_archival_batch_linkage": all(m["archival_batch_id"] == batch_id for m in memories),
                "source_l2_memories_tombstoned": bool(l2_sources)
                and all(m["archived_at"] for m in l2_sources),
                "batch_record_completed": bool(batch_row) and batch_row[2] == "completed",
            }
        )
        details.update(
            {
                "batch": {
                    "source_count": batch_row[0] if batch_row else None,
                    "l3_count": batch_row[1] if batch_row else None,
                    "status": batch_row[2] if batch_row else None,
                },
                "narrative_memory": l3_narratives[:1],
                "l2_source_count": len(l2_sources),
                "l3_memory_count": len([m for m in memories if m["tier"] == "L3"]),
                "narrative_version_count": version_count,
            }
        )
    else:
        checks.update(
            {
                "has_l3_narrative_memory": False,
                "narrative_has_version_entry": False,
                "same_archival_batch_linkage": False,
                "source_l2_memories_tombstoned": False,
                "batch_record_completed": False,
            }
        )

    payload = {
        "status": "pass" if all(checks.values()) else "fail",
        "agent_id": agent_id,
        "batch_id": batch_id,
        "checks": checks,
        "details": details,
    }
    write_json(args.results_dir / "narrative_archival.json", payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
