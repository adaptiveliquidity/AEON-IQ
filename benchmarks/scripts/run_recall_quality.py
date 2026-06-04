#!/usr/bin/env python3
"""Evaluate deterministic memory recall without judging LLM answer quality."""

from __future__ import annotations

import argparse
import time
from pathlib import Path
from typing import Any

from common import (
    AEON_BASE_URL,
    default_results_dir,
    load_dataset,
    request_json,
    write_csv,
    write_json,
)


def contains_at(results: list[dict[str, Any]], expected_id: str, k: int) -> bool:
    return any(r.get("id") == expected_id for r in results[:k])


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)

    seed_path = args.results_dir / "seed_summary.json"
    if not seed_path.exists():
        payload = {
            "status": "not_run",
            "reason": "seed_summary.json not found; run seed_memories.py first",
        }
        write_json(args.results_dir / "recall_quality.json", payload)
        write_csv(
            args.results_dir / "recall_quality.csv",
            [],
            ["query", "expected_label", "recall_at_1", "recall_at_3", "recall_at_5", "precision_at_5"],
        )
        return 0

    seed = __import__("json").loads(seed_path.read_text(encoding="utf-8"))
    dataset = load_dataset()
    agent_id = dataset["agent_id"]
    label_to_id = seed.get("labels", {})
    if seed.get("status") != "ok" or not label_to_id:
        payload = {
            "status": "not_run",
            "reason": "deterministic recall seed data is unavailable",
            "seed_status": seed.get("status"),
            "seed_reason": seed.get("reason"),
        }
        write_json(args.results_dir / "recall_quality.json", payload)
        write_csv(
            args.results_dir / "recall_quality.csv",
            [],
            ["query", "expected_label", "recall_at_1", "recall_at_3", "recall_at_5", "precision_at_5"],
        )
        return 0

    rows = []
    injected_hits = 0
    injected_total = 0
    for item in dataset["memories"]:
        expected_label = item["id_label"]
        expected_id = label_to_id.get(expected_label)
        if not expected_id:
            continue
        for idx, query in enumerate(item["expected_queries"]):
            status, payload, _ = request_json(
                "POST",
                f"{AEON_BASE_URL}/api/v1/memories/search",
                {"agent_id": agent_id, "query": query, "limit": 5, "threshold": 0.95},
                timeout=30,
            )
            results = payload.get("results", []) if 200 <= status < 300 else []
            expected_hits = sum(1 for r in results[:5] if r.get("id") == expected_id)
            session_id = f"recall-quality-{expected_label}-{idx}-{int(time.time())}"
            request_json(
                "POST",
                f"{AEON_BASE_URL}/v1/chat/completions",
                {
                    "model": "gpt-4o-mini",
                    "messages": [{"role": "user", "content": query}],
                    "stream": False,
                },
                {"x-agent-id": agent_id, "x-session-id": session_id},
                timeout=30,
            )
            time.sleep(0.25)
            log_status, logs, _ = request_json(
                "GET",
                f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/retrievals?session_id={session_id}&limit=1",
                None,
                timeout=15,
            )
            injected = False
            if 200 <= log_status < 300 and logs.get("retrievals"):
                injected_ids = logs["retrievals"][0].get("injected_memory_ids", [])
                injected = expected_id in injected_ids
                injected_hits += 1 if injected else 0
                injected_total += 1

            rows.append(
                {
                    "query": query,
                    "expected_label": expected_label,
                    "expected_id": expected_id,
                    "top_result_id": results[0]["id"] if results else "",
                    "recall_at_1": contains_at(results, expected_id, 1),
                    "recall_at_3": contains_at(results, expected_id, 3),
                    "recall_at_5": contains_at(results, expected_id, 5),
                    "precision_at_5": round(expected_hits / 5.0, 3),
                    "injected_expected_memory": injected,
                    "result_count": len(results),
                    "status_code": status,
                }
            )

    totals = {
        "queries": len(rows),
        "recall_at_1": round(sum(1 for r in rows if r["recall_at_1"]) / len(rows), 4) if rows else 0,
        "recall_at_3": round(sum(1 for r in rows if r["recall_at_3"]) / len(rows), 4) if rows else 0,
        "recall_at_5": round(sum(1 for r in rows if r["recall_at_5"]) / len(rows), 4) if rows else 0,
        "precision_at_5": round(sum(float(r["precision_at_5"]) for r in rows) / len(rows), 4)
        if rows
        else 0,
        "injected_expected_memory_rate": round(injected_hits / injected_total, 4)
        if injected_total
        else 0,
    }
    write_csv(
        args.results_dir / "recall_quality.csv",
        rows,
        [
            "query",
            "expected_label",
            "expected_id",
            "top_result_id",
            "recall_at_1",
            "recall_at_3",
            "recall_at_5",
            "precision_at_5",
            "injected_expected_memory",
            "result_count",
            "status_code",
        ],
    )
    write_json(
        args.results_dir / "recall_quality.json",
        {
            "status": "ok",
            "method": "Search/retrieval-log IDs are compared to deterministic seeded labels; LLM answer text is ignored.",
            "summary": totals,
            "rows": rows,
        },
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
