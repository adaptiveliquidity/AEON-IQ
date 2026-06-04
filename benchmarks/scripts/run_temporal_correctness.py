#!/usr/bin/env python3
"""Verify temporal memory endpoints with deterministic create/update/archive operations."""

from __future__ import annotations

import argparse
import time
from pathlib import Path

from common import AEON_BASE_URL, default_results_dir, request_json, utc_now, write_json


def sleep_boundary() -> str:
    time.sleep(1.1)
    return utc_now()


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)
    agent_id = "bench-temporal"

    request_json("DELETE", f"{AEON_BASE_URL}/api/v1/agents/{agent_id}", None, timeout=15)
    t0 = utc_now()
    time.sleep(0.4)

    status, created, _ = request_json(
        "POST",
        f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/memories",
        {"content": "Temporal benchmark fact v1: Nimbus stores memories in Postgres.", "memory_type": "semantic"},
        timeout=30,
    )
    memory_id = created.get("id")
    t1 = sleep_boundary()

    patch_status, patch_payload, _ = request_json(
        "PATCH",
        f"{AEON_BASE_URL}/api/v1/memories/{memory_id}",
        {"content": "Temporal benchmark fact v2: Nimbus stores vector memories in Postgres with pgvector."},
        timeout=30,
    )
    t2 = sleep_boundary()

    status_status, status_payload, _ = request_json(
        "PATCH",
        f"{AEON_BASE_URL}/api/v1/memories/{memory_id}/status",
        {"status": "candidate", "reason": "benchmark status transition"},
        timeout=30,
    )
    t_status = sleep_boundary()

    archive_status, archive_payload, _ = request_json(
        "POST",
        f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/memories/bulk",
        {"action": "archive", "filter": {"memory_type": "semantic"}},
        timeout=30,
    )
    t3 = sleep_boundary()

    def at(ts: str):
        return request_json(
            "GET",
            f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/memories/at?timestamp={ts}&limit=20&offset=0",
            None,
            timeout=20,
        )[1]

    def diff(start: str, end: str):
        return request_json(
            "GET",
            f"{AEON_BASE_URL}/api/v1/agents/{agent_id}/memories/diff?from={start}&to={end}",
            None,
            timeout=20,
        )[1]

    snap_before = at(t0)
    snap_after_create = at(t1)
    snap_after_update = at(t2)
    snap_after_status = at(t_status)
    snap_after_archive = at(t3)
    diff_added = diff(t0, t1)
    diff_modified = diff(t1, t2)
    diff_status = diff(t2, t_status)
    diff_archived = diff(t_status, t3)
    versions = request_json(
        "GET",
        f"{AEON_BASE_URL}/api/v1/memories/{memory_id}/versions",
        None,
        timeout=20,
    )[1]

    checks = {
        "create_http_ok": 200 <= status < 300 and bool(memory_id),
        "patch_http_ok": 200 <= patch_status < 300,
        "status_http_ok": 200 <= status_status < 300,
        "archive_http_ok": 200 <= archive_status < 300,
        "snapshot_before_creation_excludes_memory": snap_before.get("total") == 0,
        "snapshot_after_creation_includes_v1": any(
            memory_id == m.get("id") and "v1" in m.get("content", "")
            for m in snap_after_create.get("memories", [])
        ),
        "snapshot_after_update_includes_latest_version": any(
            memory_id == m.get("id") and "v2" in m.get("content", "")
            for m in snap_after_update.get("memories", [])
        ),
        "snapshot_after_status_records_candidate": any(
            memory_id == m.get("id") and m.get("status") == "candidate"
            for m in snap_after_status.get("memories", [])
        ),
        "snapshot_after_archive_excludes_memory": not any(
            memory_id == m.get("id") for m in snap_after_archive.get("memories", [])
        ),
        "diff_reports_added": diff_added.get("summary", {}).get("added", 0) >= 1,
        "diff_reports_modified": diff_modified.get("summary", {}).get("modified", 0) >= 1,
        "diff_reports_status_changed": diff_status.get("summary", {}).get("status_changed", 0) >= 1,
        "diff_reports_archived": diff_archived.get("summary", {}).get("archived", 0) >= 1,
        "versions_include_initial_patch_status": versions.get("total", 0) >= 3,
    }
    payload = {
        "status": "pass" if all(checks.values()) else "fail",
        "agent_id": agent_id,
        "memory_id": memory_id,
        "timestamps": {"before": t0, "created": t1, "updated": t2, "status": t_status, "archived": t3},
        "checks": checks,
        "response_excerpts": {
            "create_response": created,
            "patch_response": {"status": patch_status, "body": patch_payload},
            "status_response": {"status": status_status, "body": status_payload},
            "archive_response": {"status": archive_status, "body": archive_payload},
            "after_create": snap_after_create.get("memories", [])[:2],
            "after_update": snap_after_update.get("memories", [])[:2],
            "after_archive": snap_after_archive.get("memories", [])[:2],
            "diff_added_summary": diff_added.get("summary"),
            "diff_modified_summary": diff_modified.get("summary"),
            "diff_status_summary": diff_status.get("summary"),
            "diff_archived_summary": diff_archived.get("summary"),
            "versions_total": versions.get("total"),
        },
    }
    write_json(args.results_dir / "temporal_correctness.json", payload)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
