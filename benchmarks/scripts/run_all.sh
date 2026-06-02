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

PY_DEPS_DIR="$RESULTS_DIR/python-deps"
export PYTHONPATH="$PY_DEPS_DIR${PYTHONPATH:+:$PYTHONPATH}"

python_deps_ready() {
  python3 -c "import psycopg, tiktoken" >/dev/null 2>&1
}

try_install_host_python_deps() {
  command -v python3 >/dev/null 2>&1 || return 1
  mkdir -p "$PY_DEPS_DIR"
  if python_deps_ready; then
    return 0
  fi
  if python3 -m pip --version >/dev/null 2>&1; then
    python3 -m pip install --quiet --target "$PY_DEPS_DIR" -r benchmarks/requirements.txt && python_deps_ready
    return $?
  fi
  if python3 -m ensurepip --user >/dev/null 2>&1; then
    python3 -m pip install --quiet --target "$PY_DEPS_DIR" -r benchmarks/requirements.txt && python_deps_ready
    return $?
  fi
  return 1
}

run_host_python_suite() {
  local failed=0
  local scripts=(
    "benchmarks/seed/seed_memories.py"
    "benchmarks/scripts/run_latency.py"
    "benchmarks/scripts/run_token_savings.py"
    "benchmarks/scripts/run_recall_quality.py"
    "benchmarks/scripts/run_temporal_correctness.py"
    "benchmarks/scripts/run_narrative_archival.py"
  )
  for script in "${scripts[@]}"; do
    python3 "$script" --results-dir "$RESULTS_DIR" || failed=1
  done
  return "$failed"
}

docker_results_dir() {
  if [[ "$RESULTS_DIR" == "$ROOT"* ]]; then
    printf "/repo%s" "${RESULTS_DIR#"$ROOT"}"
  else
    printf "%s" "$RESULTS_DIR"
  fi
}

benchmark_python_image() {
  if [[ -n "${BENCHMARK_PYTHON_IMAGE:-}" ]]; then
    printf "%s" "$BENCHMARK_PYTHON_IMAGE"
    return
  fi
  if docker image inspect python:3.12-slim >/dev/null 2>&1; then
    printf "python:3.12-slim"
    return
  fi
  if docker image inspect python:3.11-slim >/dev/null 2>&1; then
    printf "python:3.11-slim"
    return
  fi
  printf "python:3.12-slim"
}

run_docker_python_suite() {
  command -v docker >/dev/null 2>&1 || return 1
  local docker_results
  docker_results="$(docker_results_dir)"
  local python_image
  python_image="$(benchmark_python_image)"
  local extra_mounts=()
  if [[ "$RESULTS_DIR" != "$ROOT"* ]]; then
    extra_mounts=(-v "$RESULTS_DIR:$RESULTS_DIR")
  fi
  docker run --rm --network host \
    -v "$ROOT:/repo" "${extra_mounts[@]}" -w /repo \
    -e BENCHMARK_RESULTS_DIR="$docker_results" \
    -e AEON_BASE_URL="$AEON_BASE_URL" \
    -e MOCK_BASE_URL="$MOCK_BASE_URL" \
    -e DATABASE_URL="$DATABASE_URL" \
    -e MANAGEMENT_API_KEY="$MANAGEMENT_API_KEY" \
    -e MOCK_EMBEDDING_MODE="$MOCK_EMBEDDING_MODE" \
    -e MOCK_ARCHIVAL_COMPACTION="$MOCK_ARCHIVAL_COMPACTION" \
    -e ALLOW_UNAUTH_MANAGEMENT="$ALLOW_UNAUTH_MANAGEMENT" \
    "$python_image" bash -lc '
      python -m pip install --quiet -r benchmarks/requirements.txt
      failed=0
      for script in \
        benchmarks/seed/seed_memories.py \
        benchmarks/scripts/run_latency.py \
        benchmarks/scripts/run_token_savings.py \
        benchmarks/scripts/run_recall_quality.py \
        benchmarks/scripts/run_temporal_correctness.py \
        benchmarks/scripts/run_narrative_archival.py
      do
        python "$script" --results-dir "$BENCHMARK_RESULTS_DIR" || failed=1
      done
      exit "$failed"
    '
}

run_python_suite() {
  if try_install_host_python_deps; then
    run_host_python_suite || echo "one or more Python benchmark scripts failed; summary will mark missing or failed artifacts"
    return
  fi

  echo "host Python benchmark dependencies unavailable; trying Docker Python runner"
  if run_docker_python_suite; then
    return
  fi

  echo "Docker Python runner failed or is unavailable; falling back to host Python best effort"
  run_host_python_suite || echo "one or more Python benchmark scripts failed; summary will mark missing or failed artifacts"
}

run_k6_suite() {
  if command -v k6 >/dev/null 2>&1; then
    k6 run -e AEON_BASE_URL="$AEON_BASE_URL" benchmarks/k6/proxy_latency.js \
      --summary-export "$RESULTS_DIR/k6_proxy_latency.json" || true
    k6 run -e AEON_BASE_URL="$AEON_BASE_URL" benchmarks/k6/retrieval_latency.js \
      --summary-export "$RESULTS_DIR/k6_retrieval_latency.json" || true
    return
  fi

  if command -v docker >/dev/null 2>&1; then
    local docker_results
    docker_results="$(docker_results_dir)"
    local extra_mounts=()
    if [[ "$RESULTS_DIR" != "$ROOT"* ]]; then
      extra_mounts=(-v "$RESULTS_DIR:$RESULTS_DIR")
    fi
    docker run --rm --network host \
      -v "$ROOT:/repo" "${extra_mounts[@]}" -w /repo \
      -e AEON_BASE_URL="$AEON_BASE_URL" \
      grafana/k6 run benchmarks/k6/proxy_latency.js \
      --summary-export "$docker_results/k6_proxy_latency.json" || true
    docker run --rm --network host \
      -v "$ROOT:/repo" "${extra_mounts[@]}" -w /repo \
      -e AEON_BASE_URL="$AEON_BASE_URL" \
      grafana/k6 run benchmarks/k6/retrieval_latency.js \
      --summary-export "$docker_results/k6_retrieval_latency.json" || true
  fi

  if [[ ! -f "$RESULTS_DIR/k6_proxy_latency.json" && ! -f "$RESULTS_DIR/k6_retrieval_latency.json" ]]; then
    printf '{"status":"not_run","reason":"k6 not found and Docker k6 fallback did not produce artifacts"}\n' > "$RESULTS_DIR/k6.json"
  fi
}

if command -v docker >/dev/null 2>&1; then
  docker compose -f docker-compose.test.yml up --build -d
else
  echo "docker not found; scripts will record service-level failures if AEON-IQ is not already running"
fi

run_python_suite
run_k6_suite
python3 benchmarks/scripts/summarize_results.py --results-dir "$RESULTS_DIR"

echo "Benchmark summary: $RESULTS_DIR/summary.json"
