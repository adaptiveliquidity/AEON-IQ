# AEON-IQ v0.1.0 Benchmark Report

## Summary

This report defines the reproducible benchmark/proof package for AEON-IQ v0.1.0.
Generated raw results are written to `benchmarks/results/<timestamp>/` and are
not committed by default. Public claims should cite a specific result folder or
a curated future report, not unmeasured targets.

| Metric | Result | Notes |
|---|---:|---|
| Proxy latency overhead | Scripted | `benchmarks/scripts/run_latency.py` compares direct mock vs proxy |
| Retrieval latency overhead | Scripted | 100 and 1,000 memory seeds by default; 10,000 optional |
| Estimated token reduction | Scripted | Requires `tiktoken`; benchmark-dataset specific |
| Recall quality | Scripted | Scores retrieved/injected memory IDs, not LLM prose |
| Temporal correctness | Scripted; local run found failure | Content patch/version checks failed with backend 500 in validation |
| Narrative archival correctness | Scripted | Requires `psycopg` to seed and verify DB lineage |

Latest validation in this implementation environment wrote ignored local results
under `benchmarks/results/20260602T005931Z/`. Proxy latency and HTTP-level
retrieval scripts ran. `psycopg`, `tiktoken`, `pip`, `ensurepip`, PowerShell,
and `k6` were not available, so seed/token/narrative/k6 checks were recorded as
`not_run`. Temporal create/status/archive checks ran, but content patching failed
with `PATCH /api/v1/memories/:id` returning:

```text
error occurred while decoding column 0: mismatched types; Rust type `i64` (as SQL type `INT8`) is not compatible with SQL type `INT4`
```

## Environment

Each run writes `environment.json` containing git commit, OS, CPU/RAM where
available, Docker versions, Postgres image, seed memory counts, AMP/RMK flags,
and upstream mode.

## Methodology

- Primary latency baseline is the deterministic mock upstream at
  `mock_openai_server.py`.
- Benchmark compose uses local Postgres/pgvector and mock embeddings; no live
  provider calls are required.
- Recall quality uses deterministic memory labels from
  `benchmarks/seed/benchmark_dataset.json` and compares returned memory IDs.
- Token savings are estimated with `tiktoken` on fixed benchmark prompts and
  should be described as "estimated token reduction on this benchmark dataset."
- Narrative archival uses mock compaction output unless live extractor testing
  is explicitly enabled outside the default suite.

## Results

### Proxy latency overhead

Run:

```bash
python3 benchmarks/scripts/run_latency.py --results-dir benchmarks/results/manual
```

The script writes `latency.csv` with request count, error rate, p50/p90/p95/p99,
mean/min/max, and overhead vs direct mock upstream.

### Retrieval latency overhead

`run_latency.py` also writes `retrieval_latency.csv` for management semantic
search at seeded memory counts. Proxy retrieval timing from
`memory_retrieval_logs.latency_ms` is included in `latency.json`.

### Token savings

Run:

```bash
python3 benchmarks/scripts/run_token_savings.py --results-dir benchmarks/results/manual
```

Output:

| Scenario | Baseline tokens | AEON-IQ tokens | Delta | % savings |
|---|---:|---:|---:|---:|
| Generated per run | See `token_savings.csv` | See `token_savings.csv` | See `token_savings.csv` | See `token_savings.csv` |

### Recall quality

Run:

```bash
python3 benchmarks/scripts/run_recall_quality.py --results-dir benchmarks/results/manual
```

The report includes recall@1, recall@3, recall@5, precision@5, retrieval hit
rate, and injected expected memory checks.

### Temporal correctness

Run:

```bash
python3 benchmarks/scripts/run_temporal_correctness.py --results-dir benchmarks/results/manual
```

The JSON result records pass/fail checks and excerpts for snapshots and diffs.

### Narrative archival correctness

Run:

```bash
python3 benchmarks/scripts/run_narrative_archival.py --results-dir benchmarks/results/manual
```

The JSON result verifies `narrative_count`, L3 narrative presence, version
history, batch linkage, and L2 tombstoning.

## Claims Supported

| Claim | Supported? | Evidence |
|---|---:|---|
| AEON-IQ can be benchmarked locally with Docker and mock upstream | Yes | `docker-compose.test.yml`, `benchmarks/scripts/run_all.sh` |
| Recall quality can be measured without LLM answer judging | Yes | `run_recall_quality.py` compares deterministic memory IDs |
| Temporal memory endpoints can be proof-tested reproducibly | Partially | `run_temporal_correctness.py`; local run exposed PATCH/versioning failure |
| Narrative archival DB shape can be proof-tested reproducibly | Yes, dependency-gated | `run_narrative_archival.py` requires `psycopg` |
| Token reduction depends on workload | Yes | `run_token_savings.py` includes cases where AEON-IQ may use more tokens |

## Claims Not Yet Supported

| Claim | Reason | Next proof needed |
|---|---|---|
| `<5ms overhead` | No committed benchmark result currently proves it | Publish a curated `latency.csv` from representative hardware |
| Universal token savings or "zero token bloat" | Token impact varies by prompt/history/retrieval count | Publish dataset-specific token tables and avoid universal wording |
| Production-scale retrieval latency at 10,000+ memories | 10,000-memory seed is optional and hardware-sensitive | Curate results across multiple machines and DB settings |
| Live-provider archival quality | Default suite uses mock compaction | Add optional env-gated live extractor run and disclose provider/model/cost |
| Temporal content modification/version correctness | Local validation hit backend 500 on PATCH | Fix the `INT8`/`INT4` decode mismatch and rerun `temporal_correctness.json` |

## Reproduce

```bash
python3 -m pip install -r benchmarks/requirements.txt
docker compose -f docker-compose.test.yml up --build -d
bash benchmarks/scripts/run_all.sh
```

PowerShell:

```powershell
.\benchmarks\scripts\run_all.ps1
```

## Known Limitations

- Default benchmark results are local, mock-upstream, and hardware-dependent.
- The suite does not hide failed checks; JSON artifacts record `fail` or
  `not_run` when dependencies or services are unavailable.
- k6 is optional and reported as `not_run` if not installed.
- Live provider tests are intentionally excluded from the default suite to avoid
  surprise cost.

## Claim Inventory

| Claim | Location | Currently supported? | Proof needed |
|---|---|---:|---|
| Proxy overhead target previously shown as `< 5 ms` | `README.md` diagram | Needs benchmark proof | `latency.csv` from deterministic and representative runs |
| Persistent memory with zero app-code changes | `README.md`, `QUICKSTART.md` | Partially supported by API shape | Integration smoke tests and SDK examples |
| Infinite memory / zero token bloat | `QUICKSTART.md` pre-change wording | No | Removed/softened; use token benchmark results only |
| Dashboard token/cost savings | Dashboard overview pre-change wording | Heuristic only | Calibrated wording; real savings require dataset-specific benchmark |
| Narrative archival creates L3 narrative memories | `README.md`, `CLAUDE.md`, `src/archival.rs` | Needs reproducible proof | `narrative_archival.json` |
| Temporal memory `/at` and `/diff` correctness | README/API docs | Needs reproducible proof | `temporal_correctness.json` |
