# AEON-IQ v0.1.0 Benchmark Report

## Summary

This report defines the reproducible benchmark/proof package for AEON-IQ v0.1.0.
Generated raw results are written to `benchmarks/results/<timestamp>/` and are
not committed by default. Public claims should cite a specific result folder or
a curated future report, not unmeasured targets.

Latest local validation wrote ignored results under
`benchmarks/results/20260602T012306Z/` on WSL2 with Docker Compose v5.1.0,
Postgres/pgvector, and the deterministic mock upstream. The main proof summary
reported `pass`. Optional k6 load testing reported `fail` for the retrieval k6
script because those endpoints returned non-2xx responses under that k6 script.

| Area | Latest result | Evidence |
|---|---:|---|
| Proxy latency overhead | Pass | `latency.json`, `latency.csv` |
| Retrieval latency overhead | Pass | `retrieval_latency.csv` |
| Estimated token reduction | Pass | `token_savings.json`, `token_savings.csv` |
| Recall quality | Pass | `recall_quality.json`, deterministic memory IDs |
| Temporal correctness | Pass | `temporal_correctness.json` |
| Narrative archival correctness | Pass | `narrative_archival.json` |
| k6 proxy script | Pass, optional | `k6_proxy_latency.json` |
| k6 retrieval script | Fail, optional | `k6_retrieval_latency.json` |
| PowerShell runner | Not run here | `pwsh: command not found` in this WSL environment |

## Environment

The latest summary recorded:

| Field | Value |
|---|---|
| Result folder | `benchmarks/results/20260602T012306Z/` |
| Branch | `benchmarks/v0.1.0-proof` |
| Result-run git commit | `cdde86a63bf65fec6e14436b87fb6a01cc2dfd4f` |
| OS | `Linux-5.15.167.4-microsoft-standard-WSL2-x86_64-with-glibc2.39` |
| RAM | `15Gi` |
| Docker | `Docker version 29.3.0, build 5927d80` |
| Docker Compose | `Docker Compose version v5.1.0` |
| Postgres image | `pgvector/pgvector:pg16` |
| Seed memory counts | `100`, `1000` |
| AMP/RMK | Disabled |
| Upstream mode | Deterministic mock |

Host Python lacked `pip` and `ensurepip`; the Bash runner used an existing local
`python:3.11-slim` Docker image to install `psycopg[binary]` and `tiktoken`
inside a temporary container. k6 was not installed on the host, so the runner
used the Docker k6 image.

## Results

### Latency

`run_latency.py` compares direct mock upstream latency with AEON-IQ proxy
latency. The latest local mock run produced:

| Scenario | Mean | p95 | Mean overhead vs direct | Error rate |
|---|---:|---:|---:|---:|
| Direct mock upstream | 0.571 ms | 0.784 ms | n/a | 0.0% |
| Proxy, empty memory | 3.186 ms | 5.428 ms | 2.615 ms | 0.0% |
| Proxy, seeded memory | 4.138 ms | 5.198 ms | 3.567 ms | 0.0% |
| Proxy, seeded with retrieval log | 4.151 ms | 4.607 ms | 3.580 ms | 0.0% |

These are local WSL/mock measurements, not universal latency claims.

### Token Savings

`run_token_savings.py` uses `tiktoken` and fixed benchmark prompts. The latest
run shows why token claims must be dataset-specific:

| Scenario | Baseline | AEON-IQ | Delta | Savings |
|---|---:|---:|---:|---:|
| `profile_recall` | 73 | 46 | 27 | 36.99% |
| `archival_question` | 61 | 47 | 14 | 22.95% |
| `small_context_overhead` | 16 | 53 | -37 | -231.25% |

### Recall Quality

Recall scoring compares retrieved/injected memory IDs against deterministic seed
labels, not LLM answer prose. Latest summary:

| Metric | Result |
|---|---:|
| Queries | 14 |
| Recall@1 | 0.9286 |
| Recall@3 | 1.0 |
| Recall@5 | 1.0 |
| Precision@5 | 0.2 |
| Injected expected memory rate | 1.0 |

### Temporal Correctness

`run_temporal_correctness.py` passed all checks after fixing the PATCH/version
snapshot decode mismatch. The proof covers create, content patch, status patch,
bulk archive, `/memories/at`, `/memories/diff`, and version history.

### Narrative Archival

`run_narrative_archival.py` passed all checks: trigger HTTP success, non-skipped
archival, `narrative_count = 1`, L3 narrative row, version entry, shared batch
linkage, completed batch record, and tombstoned source L2 memories.

### k6

The optional k6 proxy script passed with p95 around 4.93 ms and no failed
requests. The optional k6 retrieval script failed its HTTP failure threshold:
all checks were non-2xx in that script, even though the main scripted retrieval,
temporal, and recall proofs passed. Treat this as a k6-script follow-up, not as
a blocker for the main proof suite.

## Reproduce

```bash
docker compose -f docker-compose.test.yml up --build -d
bash benchmarks/scripts/run_all.sh
```

The runner prefers host Python with `psycopg[binary]` and `tiktoken`. If host
`pip`/`ensurepip` are unavailable, it tries a local Docker Python image before
falling back to best-effort host scripts that record `not_run` artifacts.

PowerShell:

```powershell
.\benchmarks\scripts\run_all.ps1
```

PowerShell was not available in the latest WSL validation environment.

## Claims Supported

| Claim | Supported? | Evidence |
|---|---:|---|
| AEON-IQ can be benchmarked locally with Docker and mock upstream | Yes | `run_all.sh`, latest `summary.json` |
| Recall can be measured without LLM answer judging | Yes | `run_recall_quality.py` compares deterministic IDs |
| Temporal endpoints can be proof-tested reproducibly | Yes | Latest `temporal_correctness.json` passed |
| Narrative archival DB shape can be proof-tested reproducibly | Yes | Latest `narrative_archival.json` passed |
| Token reduction depends on workload | Yes | `small_context_overhead` uses more tokens |

## Claims Not Supported

| Claim | Reason | Next proof needed |
|---|---|---|
| Universal `<5ms overhead` | Latest local p95 overhead exceeded 4 ms in some proxy scenarios and is hardware/mock-specific | Curated multi-machine latency report |
| Universal token savings or "zero token bloat" | Benchmark includes a case where AEON-IQ uses more tokens | Dataset-specific token tables only |
| Production-scale retrieval latency at 10,000+ memories | 10,000-memory seed is optional and not in latest run | Curated 10,000+ memory results |
| Live-provider archival quality | Default suite uses mock compaction | Optional env-gated live provider run |
| k6 retrieval endpoint health | Optional k6 retrieval script failed in latest run | Fix k6 endpoint setup/script and rerun |

## Known Limitations

- Results are local, mock-upstream, and hardware-dependent.
- Generated result folders are intentionally ignored by git.
- `summary.json` reports required and dependency-gated proof failures honestly;
  optional k6 failure does not block the main proof status.
- Live provider tests are intentionally excluded from the default suite to avoid
  surprise cost.
- The latest result-run commit predates the final documentation commit; use the
  final implementation commit hash in release notes or PR metadata.
