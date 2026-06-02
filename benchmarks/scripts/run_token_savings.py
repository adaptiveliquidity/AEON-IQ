#!/usr/bin/env python3
"""Estimate prompt token reduction on the deterministic benchmark dataset."""

from __future__ import annotations

import argparse
from pathlib import Path

from common import default_results_dir, load_dataset, not_run, write_csv, write_json


def main() -> int:
    parser = argparse.ArgumentParser()
    parser.add_argument("--results-dir", type=Path, default=default_results_dir())
    args = parser.parse_args()
    args.results_dir.mkdir(parents=True, exist_ok=True)

    try:
        import tiktoken
    except Exception as exc:
        not_run(
            "token_savings",
            f"tiktoken is required for tokenizer-based counting: {exc}",
            args.results_dir,
            "token_savings.json",
        )
        write_csv(
            args.results_dir / "token_savings.csv",
            [],
            ["scenario", "baseline_tokens", "aeon_iq_tokens", "delta", "percent_savings"],
        )
        return 0

    enc = tiktoken.get_encoding("cl100k_base")
    dataset = load_dataset()
    rows = []
    for scenario in dataset["token_savings_scenarios"]:
        baseline_prompt = "\n".join(scenario["baseline_history"] + [scenario["current_message"]])
        injection = (
            "<retrieved_memories role=\"factual-reference\" trust=\"read-only\">\n"
            + "\n\n".join(scenario["relevant_memories"])
            + "\n</retrieved_memories>\n"
            + scenario["current_message"]
        )
        baseline_tokens = len(enc.encode(baseline_prompt))
        aeon_tokens = len(enc.encode(injection))
        delta = baseline_tokens - aeon_tokens
        percent = (delta / baseline_tokens * 100.0) if baseline_tokens else 0.0
        rows.append(
            {
                "scenario": scenario["name"],
                "baseline_tokens": baseline_tokens,
                "aeon_iq_tokens": aeon_tokens,
                "injected_memory_tokens": len(enc.encode("\n".join(scenario["relevant_memories"]))),
                "delta": delta,
                "percent_savings": round(percent, 2),
                "aeon_uses_more_tokens": aeon_tokens > baseline_tokens,
            }
        )

    write_csv(
        args.results_dir / "token_savings.csv",
        rows,
        [
            "scenario",
            "baseline_tokens",
            "aeon_iq_tokens",
            "injected_memory_tokens",
            "delta",
            "percent_savings",
            "aeon_uses_more_tokens",
        ],
    )
    write_json(
        args.results_dir / "token_savings.json",
        {
            "status": "ok",
            "method": "cl100k_base tokenizer; estimated prompt token reduction on benchmark dataset",
            "rows": rows,
        },
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
