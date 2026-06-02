#!/usr/bin/env python3
"""Combine benchmark artifacts into a compact summary.json."""

from __future__ import annotations

import argparse
import json
from pathlib import Path

from common import environment_metadata, write_json


def read_json(path: Path):
    if not path.exists():
        return {"status": "not_run", "reason": f"{path.name} was not produced"}
    try:
        return json.loads(path.read_text(encoding="utf-8"))
    except Exception as exc:
        return {"status": "error", "reason": str(exc)}


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, required=True)
    args = parser.parse_args()

    seed = read_json(args.results_dir / "seed_summary.json")
    seed_counts = seed.get("seed_memory_counts", []) if isinstance(seed, dict) else []
    environment = environment_metadata(seed_counts)
    write_json(args.results_dir / "environment.json", environment)

    summary = {
        "status": "ok",
        "environment": environment,
        "artifacts": {
            "seed": read_json(args.results_dir / "seed_summary.json"),
            "latency": read_json(args.results_dir / "latency.json"),
            "token_savings": read_json(args.results_dir / "token_savings.json"),
            "recall_quality": read_json(args.results_dir / "recall_quality.json"),
            "temporal_correctness": read_json(args.results_dir / "temporal_correctness.json"),
            "narrative_archival": read_json(args.results_dir / "narrative_archival.json"),
        },
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
