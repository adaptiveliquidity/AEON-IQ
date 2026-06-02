#!/usr/bin/env python3
"""Combine benchmark artifacts into a compact summary.json."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from common import environment_metadata, write_json

REQUIRED_ARTIFACTS = ("latency", "temporal_correctness")
DEPENDENCY_GATED_ARTIFACTS = ("seed", "token_savings", "recall_quality", "narrative_archival")
OPTIONAL_ARTIFACTS = ("k6",)


def read_json(path: Path):
    if not path.exists():
        return {"status": "not_run", "reason": f"{path.name} was not produced"}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        return {"status": "error", "reason": str(exc)}


def read_k6_export(path: Path) -> dict:
    artifact = read_json(path)
    if artifact.get("status") in {"not_run", "error"}:
        return artifact

    metrics = artifact.get("metrics", {}) if isinstance(artifact, dict) else {}
    checks = metrics.get("checks", {}) if isinstance(metrics, dict) else {}
    http_req_failed = metrics.get("http_req_failed", {}) if isinstance(metrics, dict) else {}
    check_failures = int(checks.get("fails", 0) or 0)
    http_failure_rate = float(http_req_failed.get("value", 0.0) or 0.0)
    artifact["status"] = "fail" if check_failures > 0 or http_failure_rate > 0 else "pass"
    artifact["check_failures"] = check_failures
    artifact["http_failure_rate"] = http_failure_rate
    return artifact


def read_k6(results_dir: Path) -> dict:
    aggregate = results_dir / "k6.json"
    if aggregate.exists():
        return read_json(aggregate)

    proxy = results_dir / "k6_proxy_latency.json"
    retrieval = results_dir / "k6_retrieval_latency.json"
    if proxy.exists() or retrieval.exists():
        proxy_artifact = read_k6_export(proxy)
        retrieval_artifact = read_k6_export(retrieval)
        statuses = {proxy_artifact.get("status"), retrieval_artifact.get("status")}
        return {
            "status": "fail" if "fail" in statuses else "pass",
            "proxy_latency": proxy_artifact,
            "retrieval_latency": retrieval_artifact,
        }

    return {"status": "not_run", "reason": "k6 artifacts were not produced"}


def artifact_status(artifact: object) -> str:
    if not isinstance(artifact, dict):
        return "error"
    return str(artifact.get("status", "unknown"))


def summarize_status(artifacts: dict[str, dict]) -> dict[str, object]:
    failures: list[dict[str, str]] = []
    not_run: list[dict[str, str]] = []

    for name in REQUIRED_ARTIFACTS:
        status = artifact_status(artifacts.get(name))
        if status in {"ok", "pass"}:
            continue
        failures.append(
            {
                "artifact": name,
                "status": status,
                "reason": str(artifacts.get(name, {}).get("reason", "required proof did not pass")),
            }
        )

    for name in DEPENDENCY_GATED_ARTIFACTS:
        status = artifact_status(artifacts.get(name))
        if status in {"ok", "pass"}:
            continue
        if status == "not_run":
            not_run.append(
                {
                    "artifact": name,
                    "reason": str(artifacts.get(name, {}).get("reason", "dependency-gated proof was not run")),
                }
            )
            continue
        failures.append(
            {
                "artifact": name,
                "status": status,
                "reason": str(artifacts.get(name, {}).get("reason", "dependency-gated proof failed")),
            }
        )

    if failures:
        overall = "fail"
    elif not_run:
        overall = "partial"
    else:
        overall = "pass"

    return {
        "overall_status": overall,
        "failures": failures,
        "not_run": not_run,
        "required_artifacts": list(REQUIRED_ARTIFACTS),
        "dependency_gated_artifacts": list(DEPENDENCY_GATED_ARTIFACTS),
        "optional_artifacts": list(OPTIONAL_ARTIFACTS),
    }


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, required=True)
    args = parser.parse_args()

    seed = read_json(args.results_dir / "seed_summary.json")
    seed_counts = seed.get("seed_memory_counts", []) if isinstance(seed, dict) else []
    environment = environment_metadata(seed_counts)
    write_json(args.results_dir / "environment.json", environment)

    artifacts = {
        "seed": read_json(args.results_dir / "seed_summary.json"),
        "latency": read_json(args.results_dir / "latency.json"),
        "token_savings": read_json(args.results_dir / "token_savings.json"),
        "recall_quality": read_json(args.results_dir / "recall_quality.json"),
        "temporal_correctness": read_json(args.results_dir / "temporal_correctness.json"),
        "narrative_archival": read_json(args.results_dir / "narrative_archival.json"),
        "k6": read_k6(args.results_dir),
    }
    status = summarize_status(artifacts)
    summary = {
        "status": status["overall_status"],
        "overall_status": status["overall_status"],
        "environment": environment,
        "artifacts": artifacts,
        "failures": status["failures"],
        "not_run": status["not_run"],
        "required_artifacts": status["required_artifacts"],
        "dependency_gated_artifacts": status["dependency_gated_artifacts"],
        "optional_artifacts": status["optional_artifacts"],
        "notes": [
            "Results are benchmark-dataset specific and should not be generalized without fresh runs.",
            "Primary latency baseline is the deterministic mock upstream.",
            "Generated raw result folders are intentionally ignored by git.",
        ],
    }
    write_json(args.results_dir / "summary.json", summary)
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
