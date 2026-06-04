#!/usr/bin/env python3
"""Shared helpers for AEON-IQ benchmark scripts."""

from __future__ import annotations

import csv
import hashlib
import json
import math
import os
import platform
import re
import statistics
import subprocess
import time
import urllib.error
import urllib.request
from datetime import datetime, timezone
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[2]
BENCHMARKS_DIR = ROOT / "benchmarks"
DATASET_PATH = BENCHMARKS_DIR / "seed" / "benchmark_dataset.json"

AEON_BASE_URL = os.environ.get("AEON_BASE_URL", "http://localhost:8080").rstrip("/")
MOCK_BASE_URL = os.environ.get("MOCK_BASE_URL", "http://localhost:11435").rstrip("/")
DATABASE_URL = os.environ.get(
    "DATABASE_URL", "postgresql://memoryos:memoryos_secret@localhost:5432/memoryos"
)
MANAGEMENT_API_KEY = os.environ.get("MANAGEMENT_API_KEY", "sk-mock-test-key-not-real")
EMBEDDING_DIMENSION = int(os.environ.get("EMBEDDING_DIMENSION", "1536"))
BENCH_AGENT_PREFIX = "bench-"

TOKEN_RE = re.compile(r"[a-z0-9]+")
STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "for", "from", "how", "i", "in",
    "is", "it", "me", "my", "of", "on", "or", "the", "this", "to", "what",
    "with", "you", "your",
}


def utc_now() -> str:
    return datetime.now(timezone.utc).isoformat().replace("+00:00", "Z")


def default_results_dir() -> Path:
    env = os.environ.get("BENCHMARK_RESULTS_DIR")
    if env:
        path = Path(env)
    else:
        stamp = datetime.now(timezone.utc).strftime("%Y%m%dT%H%M%SZ")
        path = BENCHMARKS_DIR / "results" / stamp
    path.mkdir(parents=True, exist_ok=True)
    return path


def load_dataset() -> dict[str, Any]:
    return json.loads(DATASET_PATH.read_text(encoding="utf-8"))


def write_json(path: Path, data: Any) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    path.write_text(json.dumps(data, indent=2, sort_keys=True) + "\n", encoding="utf-8")


def write_csv(path: Path, rows: list[dict[str, Any]], fieldnames: list[str] | None = None) -> None:
    path.parent.mkdir(parents=True, exist_ok=True)
    if fieldnames is None:
        keys: list[str] = []
        for row in rows:
            for key in row:
                if key not in keys:
                    keys.append(key)
        fieldnames = keys
    with path.open("w", newline="", encoding="utf-8") as f:
        writer = csv.DictWriter(f, fieldnames=fieldnames, extrasaction="ignore")
        writer.writeheader()
        for row in rows:
            writer.writerow(row)


def run_command(args: list[str], timeout: int = 20) -> str | None:
    try:
        return subprocess.check_output(args, text=True, stderr=subprocess.DEVNULL, timeout=timeout).strip()
    except Exception:
        return None


def environment_metadata(seed_counts: list[int] | None = None) -> dict[str, Any]:
    docker_version = run_command(["docker", "--version"])
    compose_version = run_command(["docker", "compose", "version"])
    git_commit = run_command(["git", "rev-parse", "HEAD"])
    cpu = platform.processor() or platform.machine()
    return {
        "generated_at": utc_now(),
        "git_commit": git_commit,
        "os": platform.platform(),
        "python": platform.python_version(),
        "cpu": cpu,
        "ram": run_command(["bash", "-lc", "free -h | awk '/Mem:/ {print $2}'"]),
        "docker_version": docker_version,
        "docker_compose_version": compose_version,
        "postgres_image": "pgvector/pgvector:pg16",
        "seed_memory_counts": seed_counts or [],
        "amp_enabled": os.environ.get("AMP_ENABLED", "false"),
        "rmk_enabled": os.environ.get("RMK_ENABLED", "false"),
        "upstream_mode": os.environ.get("UPSTREAM_MODE", "mock"),
        "aeon_base_url": AEON_BASE_URL,
        "mock_base_url": MOCK_BASE_URL,
    }


def headers(extra: dict[str, str] | None = None) -> dict[str, str]:
    out = {"Content-Type": "application/json"}
    if MANAGEMENT_API_KEY:
        out["X-Management-Key"] = MANAGEMENT_API_KEY
    if extra:
        out.update(extra)
    return out


def request_json(
    method: str,
    url: str,
    body: dict[str, Any] | None = None,
    extra_headers: dict[str, str] | None = None,
    timeout: int = 30,
) -> tuple[int, Any, float]:
    payload = json.dumps(body).encode("utf-8") if body is not None else None
    req = urllib.request.Request(url, data=payload, headers=headers(extra_headers), method=method)
    start = time.perf_counter()
    try:
        with urllib.request.urlopen(req, timeout=timeout) as resp:
            raw = resp.read().decode("utf-8")
            elapsed_ms = (time.perf_counter() - start) * 1000
            return resp.status, json.loads(raw) if raw else {}, elapsed_ms
    except urllib.error.HTTPError as e:
        raw = e.read().decode("utf-8")
        elapsed_ms = (time.perf_counter() - start) * 1000
        try:
            parsed: Any = json.loads(raw)
        except Exception:
            parsed = raw
        return e.code, parsed, elapsed_ms


def percentile(values: list[float], pct: float) -> float | None:
    if not values:
        return None
    ordered = sorted(values)
    rank = (len(ordered) - 1) * (pct / 100.0)
    lo = math.floor(rank)
    hi = math.ceil(rank)
    if lo == hi:
        return ordered[int(rank)]
    return ordered[lo] * (hi - rank) + ordered[hi] * (rank - lo)


def latency_summary(values: list[float], errors: int = 0) -> dict[str, Any]:
    if not values:
        return {
            "request_count": 0,
            "error_count": errors,
            "error_rate": 1.0 if errors else 0.0,
            "p50_ms": None,
            "p90_ms": None,
            "p95_ms": None,
            "p99_ms": None,
            "mean_ms": None,
            "min_ms": None,
            "max_ms": None,
        }
    total = len(values) + errors
    return {
        "request_count": total,
        "error_count": errors,
        "error_rate": errors / total if total else 0.0,
        "p50_ms": round(percentile(values, 50) or 0.0, 3),
        "p90_ms": round(percentile(values, 90) or 0.0, 3),
        "p95_ms": round(percentile(values, 95) or 0.0, 3),
        "p99_ms": round(percentile(values, 99) or 0.0, 3),
        "mean_ms": round(statistics.fmean(values), 3),
        "min_ms": round(min(values), 3),
        "max_ms": round(max(values), 3),
    }


def hash_embedding(text: str, dim: int = EMBEDDING_DIMENSION) -> list[float]:
    vec = [0.0] * dim
    tokens = [t for t in TOKEN_RE.findall(text.lower()) if t not in STOPWORDS]
    if not tokens:
        tokens = ["empty"]
    for token in tokens:
        digest = hashlib.sha256(token.encode()).digest()
        idx = int.from_bytes(digest[:4], "big") % dim
        vec[idx] += 1.0
    norm = math.sqrt(sum(v * v for v in vec)) or 1.0
    return [v / norm for v in vec]


def vector_literal(values: list[float]) -> str:
    return "[" + ",".join(f"{v:.8f}" for v in values) + "]"


def not_run(area: str, reason: str, results_dir: Path, filename: str) -> dict[str, Any]:
    payload = {"area": area, "status": "not_run", "reason": reason, "generated_at": utc_now()}
    write_json(results_dir / filename, payload)
    return payload
