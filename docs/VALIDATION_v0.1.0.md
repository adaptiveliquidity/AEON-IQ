# AEON-IQ v0.1.0 Validation

This document records the v0.1.0 validation evidence. Benchmark numbers are mock-upstream measurements and are runner-dependent; use them as reproducibility proof, not universal production latency claims.

## Validation Sources

| Source | Reference | Result |
|---|---|---|
| Local full-suite validation | PR #16, merge commit `b316b19e2d8346627bbbdb9b325a3bea594d6d6e` | Full validation pass |
| CI benchmark proof | Merge commit `6f9f62d73fd4cb412ac5cad18678aa573208eab2` | `proof_status: pass` |

CI artifact:

```text
aeon-iq-benchmark-proof-6f9f62d73fd4cb412ac5cad18678aa573208eab2
```

## Result

- Benchmarks: PASS
- Full validation: PASS
- CI proof status: `pass`

## Passed Checks

- Rust fmt
- Rust clippy
- Rust unit tests
- Full Postgres-backed Rust tests
- Dashboard npm ci/lint/build
- Docker/Postgres integration
- Full-stack memory test
- k6 proxy latency
- k6 retrieval latency
- Security regressions

## Key Benchmark Numbers

Representative mock validation numbers from the v0.1.0 proof runs:

- direct_mock_upstream p95: 0.86 ms
- proxy_empty_memory p95: 4.897 ms
- proxy_seeded_memory p95: 6.554 ms
- proxy_seeded_with_retrieval_log p95: 6.448 ms
- retrieval_search_100 p95: 3.101 ms
- retrieval_search_1000 p95: 9.31 ms
- recall@1: 0.9286
- recall@3: 1.0
- recall@5: 1.0
- injected expected memory rate: 1.0
- k6 proxy latency p95: 18.88 ms
- k6 retrieval latency p95: 9.48 ms

These values come from deterministic mock validation and CI/local proof artifacts. They should be compared against future runs from the same workflow rather than cited as hardware-independent performance guarantees.

## Security Regressions

- Empty `MANAGEMENT_API_KEY` fails startup when unauthenticated management is disabled.
- Explicit local-dev unauthenticated management mode works only when allowed.
- `MAX_BODY_BYTES` oversized requests return HTTP 413.
- `stream=true` mock responses return valid `text/event-stream`.
