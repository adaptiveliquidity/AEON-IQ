#!/usr/bin/env bash
set -euo pipefail

ROOT="$(cd "$(dirname "${BASH_SOURCE[0]}")/../.." && pwd)"
STAMP="$(date -u +"%Y%m%dT%H%M%SZ")"
RESULTS_DIR="${BENCHMARK_RESULTS_DIR:-$ROOT/benchmarks/results/$STAMP}"
export BENCHMARK_RESULTS_DIR="$RESULTS_DIR"
export AEON_BASE_URL="${AEON_BASE_URL:-http://localhost:8080}"
export MOCK_BASE_URL="${MOCK_BASE_URL:-http://localhost:11435}"
export DATABASE_URL="${DATABASE_URL:-postgresql://memoryos:memoryos_secret@localhost:5432/memoryos}"
export MANAGEMENT_API_KEY="${MANAGEMENT_API_KEY:-sk-mock-test-key-not-real}"
export MOCK_EMBEDDING_MODE="${MOCK_EMBEDDING_MODE:-hash}"
export MOCK_ARCHIVAL_COMPACTION="${MOCK_ARCHIVAL_COMPACTION:-true}"
export ALLOW_UNAUTH_MANAGEMENT="${ALLOW_UNAUTH_MANAGEMENT:-true}"

mkdir -p "$RESULTS_DIR"
cd "$ROOT"

echo "AEON-IQ benchmark results: $RESULTS_DIR"

if command -v docker >/dev/null 2>&1; then
  docker compose -f docker-compose.test.yml up --build -d
else
  echo "docker not found; scripts will record service-level failures if AEON-IQ is not already running"
fi

python3 benchmarks/seed/seed_memories.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/run_latency.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/run_token_savings.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/run_recall_quality.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/run_temporal_correctness.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/run_narrative_archival.py --results-dir "$RESULTS_DIR"
python3 benchmarks/scripts/summarize_results.py --results-dir "$RESULTS_DIR"

if command -v k6 >/dev/null 2>&1; then
  k6 run -e AEON_BASE_URL="$AEON_BASE_URL" benchmarks/k6/proxy_latency.js \
    --summary-export "$RESULTS_DIR/k6_proxy_latency.json" || true
  k6 run -e AEON_BASE_URL="$AEON_BASE_URL" benchmarks/k6/retrieval_latency.js \
    --summary-export "$RESULTS_DIR/k6_retrieval_latency.json" || true
else
  printf '{"status":"not_run","reason":"k6 not found"}\n' > "$RESULTS_DIR/k6.json"
fi

echo "Benchmark summary: $RESULTS_DIR/summary.json"
