# AEON-IQ v0.1.0 Benchmark Report

## Latest CI Benchmark Proof

The current public benchmark proof is the GitHub Actions `Benchmark Proof` workflow, which uploads durable artifacts named:

```text
aeon-iq-benchmark-proof-<commit>
```

For v0.1.0, the CI benchmark proof merge commit is:

```text
6f9f62d73fd4cb412ac5cad18678aa573208eab2
```

Artifact:

```text
aeon-iq-benchmark-proof-6f9f62d73fd4cb412ac5cad18678aa573208eab2
```

The artifact summary reported `proof_status: pass`. Public benchmark claims should cite this CI artifact, or a newer `Benchmark Proof` artifact, rather than an ignored local result directory.

The workflow uses Docker Compose, Postgres/pgvector, the deterministic mock OpenAI server, Rust checks, Python compile checks, the full benchmark runner, and a proof gate that fails unless `summary.json` reports `proof_status == "pass"`.

## Scope

AEON-IQ includes a reproducible benchmark/proof suite for proxy latency, retrieval latency, estimated token reduction, recall quality, temporal memory correctness, narrative archival correctness, and optional k6 latency checks.

Benchmark numbers are mock-upstream and runner-dependent. Treat them as repeatable proof from a specific workflow run, not as universal production performance guarantees.

## Historical Local WSL2 Result

Earlier local validation wrote ignored results under `benchmarks/results/20260602T014014Z/` on WSL2 with Docker Compose v5.1.0, Postgres/pgvector, and the deterministic mock upstream. That folder is historical/local evidence only. It is useful for development comparison, but public claims should cite the CI artifact above or a newer CI artifact.

The historical local summary reported `pass`; optional k6 proxy and retrieval scripts also reported `pass`.

| Area | Evidence |
|---|---|
| Proxy latency overhead | `latency.json`, `latency.csv` |
| Retrieval latency overhead | `retrieval_latency.csv` |
| Estimated token reduction | `token_savings.json`, `token_savings.csv` |
| Recall quality | `recall_quality.json`, deterministic memory IDs |
| Temporal correctness | `temporal_correctness.json` |
| Narrative archival correctness | `narrative_archival.json` |
| k6 proxy script | `k6_proxy_latency.json` |
| k6 retrieval script | `k6_retrieval_latency.json` |

## Representative v0.1.0 Metrics

Representative mock proof numbers from v0.1.0 validation:

| Metric | Result |
|---|---:|
| direct_mock_upstream p95 | 0.86 ms |
| proxy_empty_memory p95 | 4.897 ms |
| proxy_seeded_memory p95 | 6.554 ms |
| proxy_seeded_with_retrieval_log p95 | 6.448 ms |
| retrieval_search_100 p95 | 3.101 ms |
| retrieval_search_1000 p95 | 9.31 ms |
| recall@1 | 0.9286 |
| recall@3 | 1.0 |
| recall@5 | 1.0 |
| injected expected memory rate | 1.0 |
| k6 proxy latency p95 | 18.88 ms |
| k6 retrieval latency p95 | 9.48 ms |

Use the exact artifact `summary.json` for the final numbers of any specific workflow run.

## Reproduce Locally

```bash
docker compose -f docker-compose.test.yml up --build -d
bash benchmarks/scripts/run_all.sh
```

The runner writes generated outputs under `benchmarks/results/<timestamp>/` unless `BENCHMARK_RESULTS_DIR` is set. These outputs are intentionally ignored by git.

The runner prefers host Python with `psycopg[binary]` and `tiktoken`. If host Python dependencies are unavailable, it can fall back to Docker Python where available. k6 is optional locally and can run from host k6 or Docker.

PowerShell:

```powershell
.\benchmarks\scripts\run_all.ps1
```

## Claims Supported

| Claim | Supported? | Evidence |
|---|---:|---|
| AEON-IQ has repeatable benchmark proof in CI | Yes | `Benchmark Proof` workflow artifact |
| AEON-IQ can be benchmarked locally with Docker and mock upstream | Yes | `run_all.sh`, local `summary.json` |
| Recall can be measured without LLM answer judging | Yes | `run_recall_quality.py` compares deterministic IDs |
| Temporal endpoints can be proof-tested reproducibly | Yes | `temporal_correctness.json` |
| Narrative archival DB shape can be proof-tested reproducibly | Yes | `narrative_archival.json` |
| Token reduction depends on workload | Yes | Token-savings benchmark includes overhead cases |

## Claims Not Supported

| Claim | Reason | Next proof needed |
|---|---|---|
| Universal `<5ms overhead` | Mock CI/local latency is hardware- and runner-specific | Curated multi-machine latency report |
| Universal token savings or "zero token bloat" | Benchmark includes cases where AEON-IQ can use more prompt tokens | Dataset-specific token tables only |
| Production-scale retrieval latency at 10,000+ memories | 10,000-memory seed is optional and not required by current CI proof | Curated 10,000+ memory results |
| Live-provider archival quality | Default suite uses mock extraction and compaction | Optional env-gated live provider run |
| k6 as production load proof | k6 checks are proof smoke tests, not a production load model | Curated production-like load report |

## Known Limitations

- Results are mock-upstream and runner-dependent.
- Generated result folders are intentionally ignored by git.
- `summary.json` reports required and dependency-gated proof failures honestly.
- Live provider tests are intentionally excluded from the default suite to avoid surprise cost.
- Future public performance claims should cite the exact CI artifact and commit used as evidence.
