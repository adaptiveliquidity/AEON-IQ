# AEON-IQ v0.1.0 Validation

Commit validated: b316b19e2d8346627bbbdb9b325a3bea594d6d6e
PR: #16

## Result

Benchmarks: PASS
Full validation: PASS

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

- direct_mock_upstream p95: 0.86 ms
- proxy_empty_memory p95: 4.897 ms
- proxy_seeded_memory p95: 6.554 ms
- proxy_seeded_with_retrieval_log p95: 6.448 ms
- retrieval_search_100 p95: 3.101 ms
- retrieval_search_1000 p95: 9.31 ms
- recall@1: 0.9286
- recall@3: 1.0
- recall@5: 1.0
- k6 proxy latency p95: 18.88 ms
- k6 retrieval latency p95: 9.48 ms

## Security Regressions

- Empty MANAGEMENT_API_KEY fails startup.
- Explicit dev unauth mode works only when allowed.
- MAX_BODY_BYTES oversized requests return 413.
- Mock stream=true returns valid SSE.
