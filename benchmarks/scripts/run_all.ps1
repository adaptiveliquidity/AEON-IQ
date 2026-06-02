param(
  [string]$ResultsDir = $env:BENCHMARK_RESULTS_DIR
)

$ErrorActionPreference = "Stop"
$Root = Resolve-Path (Join-Path $PSScriptRoot "..\..")
if (-not $ResultsDir) {
  $Stamp = (Get-Date).ToUniversalTime().ToString("yyyyMMddTHHmmssZ")
  $ResultsDir = Join-Path $Root "benchmarks\results\$Stamp"
}

$env:BENCHMARK_RESULTS_DIR = $ResultsDir
$env:AEON_BASE_URL = if ($env:AEON_BASE_URL) { $env:AEON_BASE_URL } else { "http://localhost:8080" }
$env:MOCK_BASE_URL = if ($env:MOCK_BASE_URL) { $env:MOCK_BASE_URL } else { "http://localhost:11435" }
$env:DATABASE_URL = if ($env:DATABASE_URL) { $env:DATABASE_URL } else { "postgresql://memoryos:memoryos_secret@localhost:5432/memoryos" }
$env:MANAGEMENT_API_KEY = if ($env:MANAGEMENT_API_KEY) { $env:MANAGEMENT_API_KEY } else { "sk-mock-test-key-not-real" }
$env:MOCK_EMBEDDING_MODE = if ($env:MOCK_EMBEDDING_MODE) { $env:MOCK_EMBEDDING_MODE } else { "hash" }
$env:MOCK_ARCHIVAL_COMPACTION = if ($env:MOCK_ARCHIVAL_COMPACTION) { $env:MOCK_ARCHIVAL_COMPACTION } else { "true" }
$env:ALLOW_UNAUTH_MANAGEMENT = if ($env:ALLOW_UNAUTH_MANAGEMENT) { $env:ALLOW_UNAUTH_MANAGEMENT } else { "true" }

New-Item -ItemType Directory -Force -Path $ResultsDir | Out-Null
Set-Location $Root
Write-Host "AEON-IQ benchmark results: $ResultsDir"

if (Get-Command docker -ErrorAction SilentlyContinue) {
  docker compose -f docker-compose.test.yml up --build -d
} else {
  Write-Host "docker not found; scripts will record service-level failures if AEON-IQ is not already running"
}

python benchmarks/seed/seed_memories.py --results-dir $ResultsDir
python benchmarks/scripts/run_latency.py --results-dir $ResultsDir
python benchmarks/scripts/run_token_savings.py --results-dir $ResultsDir
python benchmarks/scripts/run_recall_quality.py --results-dir $ResultsDir
python benchmarks/scripts/run_temporal_correctness.py --results-dir $ResultsDir
python benchmarks/scripts/run_narrative_archival.py --results-dir $ResultsDir
python benchmarks/scripts/summarize_results.py --results-dir $ResultsDir

if (Get-Command k6 -ErrorAction SilentlyContinue) {
  k6 run -e AEON_BASE_URL=$env:AEON_BASE_URL benchmarks/k6/proxy_latency.js --summary-export "$ResultsDir\k6_proxy_latency.json"
  k6 run -e AEON_BASE_URL=$env:AEON_BASE_URL benchmarks/k6/retrieval_latency.js --summary-export "$ResultsDir\k6_retrieval_latency.json"
} else {
  '{"status":"not_run","reason":"k6 not found"}' | Out-File -Encoding utf8 "$ResultsDir\k6.json"
}

Write-Host "Benchmark summary: $ResultsDir\summary.json"
