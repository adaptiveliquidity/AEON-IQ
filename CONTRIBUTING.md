# Contributing

## Workflow

Branch from `main`, keep pull requests focused, and avoid mixing product changes with documentation or generated-output updates.

Before opening a PR, run:

```bash
cargo fmt --check
cargo clippy -- -D warnings
cargo test -- --skip memory::store::tests
python3 -m compileall -q benchmarks mock_openai_server.py run_tests.py test_memory.py
```

For database, migration, retrieval, archival, or integration changes, also run the Docker/Postgres-backed tests:

```bash
docker compose -f docker-compose.test.yml up --build -d
python3 test_memory.py
DATABASE_URL=postgresql://memoryos:memoryos_secret@localhost:5432/memoryos cargo test
```

Benchmark-affecting changes should preserve the GitHub Actions `Benchmark Proof` workflow and should pass with `proof_status: pass`.

Do not commit generated artifacts, including `.venv/`, `test-artifacts/`, `target/`, `node_modules/`, `ci-artifacts/`, or `benchmarks/results/`.

Wait for CI and Benchmark Proof to pass before merge.
