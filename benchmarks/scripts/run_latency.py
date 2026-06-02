#!/usr/bin/env python3
"""Measure proxy and retrieval latency against the deterministic mock upstream."""

from __future__ import annotations

import argparse
import json
import time
from pathlib import Path
from typing import Any

from common import (
    AEON_BASE_URL,
    MOCK_BASE_URL,
    default_results_dir,
    latency_summary,
    request_json,
    write_csv,
    write_json,
)


def chat_body(message: str) -> dict[str, Any]:
    return {
        "model": "gpt-4o-mini",
        "messages": [{"role": "user", "content": message}],
        "stream": False,
    }


def run_chat_scenario(name: str, url: str, agent_id: str | None, requests: int) -> dict[str, Any]:
    latencies: list[float] = []
    errors = 0
    for i in range(requests):
        headers = {}
        if agent_id:
            headers = {"x-agent-id": agent_id, "x-session-id": f"{name}-{i}"}
        status, _, elapsed = request_json(
            "POST",
            url,
            chat_body(f"Benchmark latency probe {i}. What do you know about topic {i % 10}?"),
            headers,
            timeout=30,
        )
        if 200 <= status < 300:
            latencies.append(elapsed)
        else:
            errors += 1
    summary = latency_summary(latencies, errors)
    summary["scenario"] = name
    return summary


def run_search_scenario(agent_id: str, count: int, requests: int) -> dict[str, Any]:
    latencies: list[float] = []
    result_counts: list[int] = []
    errors = 0
    for i in range(requests):
        status, payload, elapsed = request_json(
            "POST",
            f"{AEON_BASE_URL}/api/v1/memories/search",
            {
                "agent_id": agent_id,
                "query": f"Nimbus vector probe topic {i % 25}",
                "limit": 5,
                "threshold": 0.95,
            },
            timeout=30,
        )
        if 200 <= status < 300:
            latencies.append(elapsed)
            result_counts.append(len(payload.get("results", [])))
        else:
            errors += 1
    summary = latency_summary(latencies, errors)
    summary.update(
        {
            "scenario": f"retrieval_search_{count}",
            "seed_memory_count": count,
            "mean_result_count": round(sum(result_counts) / len(result_counts), 3)
            if result_counts
            else 0,
        }
    )
    return summary


def run_proxy_retrieval_logs(agent_id: str, requests: int) -> dict[str, Any]:
    sessions = [f"retrieval-log-{int(time.time())}-{i}" for i in range(requests)]
    proxy_latencies: list[float] = []
    errors = 0
    for i, session_id in enumerate(sessions):
        status, _, elapsed = request_json(
            "POST",
            f"{AEON_BASE_URL}/v1/chat/completions",
            chat_body(f"Recall Nimbus vector probe topic {i % 25}."),
            {"x-agent-id": agent_id, "x-session-id": session_id},
            timeout=30,
        )
        if 200 <= status < 300:
            proxy_latencies.append(elapsed)
        else:
            errors += 1

    time.sleep(1.0)
    retrieval_latencies: list[float] = []
    injected_counts: list[int] = []
    candidate_counts: list[int] = []
    for session_id in sessions:
        status, payload, _ = request_json(
            "GET",
            f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/retrievals?session_id={session_id}&limit=1",
            None,
            timeout=15,
        )
        if not (200 <= status < 300):
            continue
        rows = payload.get("retrievals", [])
        if rows:
            row = rows[0]
            if row.get("latency_ms") is not None:
                retrieval_latencies.append(float(row["latency_ms"]))
            injected_counts.append(len(row.get("injected_memory_ids", [])))
            candidate_counts.append(len(row.get("candidate_memory_ids", [])))

    summary = latency_summary(proxy_latencies, errors)
    summary.update(
        {
            "scenario": "proxy_seeded_with_retrieval_log",
            "retrieval_log": latency_summary(retrieval_latencies, 0),
            "mean_injected_count": round(sum(injected_counts) / len(injected_counts), 3)
            if injected_counts
            else 0,
            "mean_candidate_count": round(sum(candidate_counts) / len(candidate_counts), 3)
            if candidate_counts
            else 0,
        }
    )
    return summary


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    parser.add_argument("--requests", type=int, default=30)
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)

    direct = run_chat_scenario(
        "direct_mock_upstream",
        f"{MOCK_BASE_URL}/v1/chat/completions",
        None,
        args.requests,
    )
    proxy_empty = run_chat_scenario(
        "proxy_empty_memory",
        f"{AEON_BASE_URL}/v1/chat/completions",
        "bench-latency-empty",
        args.requests,
    )
    proxy_seeded = run_chat_scenario(
        "proxy_seeded_memory",
        f"{AEON_BASE_URL}/v1/chat/completions",
        "bench-latency-seeded",
        args.requests,
    )
    proxy_logs = run_proxy_retrieval_logs("bench-latency-seeded", max(5, args.requests // 3))

    direct_mean = direct.get("mean_ms") or 0
    direct_p95 = direct.get("p95_ms") or 0
    for row in (proxy_empty, proxy_seeded, proxy_logs):
        row["mean_overhead_vs_direct_ms"] = round((row.get("mean_ms") or 0) - direct_mean, 3)
        row["p95_overhead_vs_direct_ms"] = round((row.get("p95_ms") or 0) - direct_p95, 3)

    seed_counts: list[int] = []
    seed_path = args.results_dir / "seed_summary.json"
    if seed_path.exists():
        try:
            seeded = json.loads(seed_path.read_text(encoding="utf-8"))
            if seeded.get("status") == "ok":
                seed_counts = seeded.get("seed_memory_counts", seed_counts)
        except Exception:
            pass

    retrieval_rows = []
    for count in seed_counts:
        retrieval_rows.append(
            run_search_scenario(
                f"bench-retrieval-{count}",
                count,
                max(3, args.requests // 4) if count >= 10000 else max(5, args.requests // 2),
            )
        )

    rows = [direct, proxy_empty, proxy_seeded, proxy_logs]
    write_csv(
        args.results_dir / "latency.csv",
        rows,
        [
            "scenario",
            "request_count",
            "error_count",
            "error_rate",
            "p50_ms",
            "p90_ms",
            "p95_ms",
            "p99_ms",
            "mean_ms",
            "min_ms",
            "max_ms",
            "mean_overhead_vs_direct_ms",
            "p95_overhead_vs_direct_ms",
        ],
    )
    write_csv(
        args.results_dir / "retrieval_latency.csv",
        retrieval_rows,
        [
            "scenario",
            "seed_memory_count",
            "request_count",
            "error_count",
            "error_rate",
            "p50_ms",
            "p95_ms",
            "p99_ms",
            "mean_ms",
            "min_ms",
            "max_ms",
            "mean_result_count",
        ],
    )
    payload = {"status": "ok", "proxy_latency": rows, "retrieval_latency": retrieval_rows}
    write_json(args.results_dir / "latency.json", payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
