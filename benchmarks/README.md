# AEON-IQ Benchmarks

This directory contains reproducible proof scripts for AEON-IQ v0.1.0 claims.
The default path uses local Docker services and the deterministic mock upstream;
live provider tests are intentionally optional and must be enabled explicitly.

## Setup

```bash
python3 -m pip install -r benchmarks/requirements.txt
docker compose -f docker-compose.test.yml up --build -d
```

If host Python has no `pip`/`ensurepip`, `run_all.sh` and `run_all.ps1` try a
temporary Docker Python runner instead. Set `BENCHMARK_PYTHON_IMAGE` to override
the image; otherwise the scripts prefer an existing local `python:3.12-slim` or
`python:3.11-slim` image before trying to pull `python:3.12-slim`.

The test compose file starts Postgres/pgvector, AEON-IQ, the dashboard, and the
mock OpenAI-compatible server. Benchmark mode sets `MOCK_EMBEDDING_MODE=hash`
and `MOCK_ARCHIVAL_COMPACTION=true` for deterministic retrieval and archival
proofs. The default local management key is `sk-mock-test-key-not-real`; set
`MANAGEMENT_API_KEY` to override it.

## Run

```bash
bash benchmarks/scripts/run_all.sh
```

PowerShell:

```powershell
.\benchmarks\scripts\run_all.ps1
```

Generated artifacts are written under `benchmarks/results/<timestamp>/` and are
ignored by git unless deliberately curated.

## Outputs

| Artifact | Purpose |
|---|---|
| `summary.json` | Combined status and links to per-area results |
| `environment.json` | Git, OS, Docker, seed counts, AMP/RMK flags, upstream mode |
| `latency.csv` | Direct mock vs proxy latency summaries |
| `retrieval_latency.csv` | Management search and retrieval latency by seed count |
| `token_savings.csv` | Tokenizer-based prompt token comparison |
| `recall_quality.csv` | Recall@k and injection hit checks by deterministic memory ID |
| `temporal_correctness.json` | Pass/fail checks for time-travel and diff endpoints |
| `narrative_archival.json` | Pass/fail checks for L3 narrative and batch lineage |

`summary.json` uses `pass`, `partial`, or `fail`:

- `pass`: required and dependency-gated proofs completed successfully.
- `partial`: required proofs passed, but dependency-gated proofs could not run.
- `fail`: a required proof, or an executed dependency-gated proof, failed.

## Individual Scripts

```bash
python3 benchmarks/seed/seed_memories.py --results-dir benchmarks/results/manual
python3 benchmarks/scripts/run_latency.py --results-dir benchmarks/results/manual
python3 benchmarks/scripts/run_token_savings.py --results-dir benchmarks/results/manual
python3 benchmarks/scripts/run_recall_quality.py --results-dir benchmarks/results/manual
python3 benchmarks/scripts/run_temporal_correctness.py --results-dir benchmarks/results/manual
python3 benchmarks/scripts/run_narrative_archival.py --results-dir benchmarks/results/manual
```

Use `BENCHMARK_INCLUDE_10000=true` or `seed_memories.py --include-10000` to add
the optional 10,000-memory retrieval seed. This can be slow on laptops.

## k6

```bash
k6 run -e AEON_BASE_URL=http://localhost:8080 benchmarks/k6/proxy_latency.js
k6 run -e AEON_BASE_URL=http://localhost:8080 benchmarks/k6/retrieval_latency.js
```

If host `k6` is absent, the full runner tries Docker `grafana/k6`. k6 artifacts
are included in `summary.json` but are optional and do not block the main proof
status. Thresholds are intentionally forgiving until a public baseline is
collected.
