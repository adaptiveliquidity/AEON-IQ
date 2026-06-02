param(
  [string]$ResultsDir = $env:BENCHMARK_RESULTS_DIR
)

$ErrorActionPreference = "Stop"
$Root = (Resolve-Path (Join-Path $PSScriptRoot "..\..")).Path
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

$PythonDepsDir = Join-Path $ResultsDir "python-deps"
$PathSeparator = [System.IO.Path]::PathSeparator
if ($env:PYTHONPATH) {
  $env:PYTHONPATH = "$PythonDepsDir$PathSeparator$env:PYTHONPATH"
} else {
  $env:PYTHONPATH = $PythonDepsDir
}

function Test-PythonDeps {
  if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    return $false
  }
  & python -c "import psycopg, tiktoken" *> $null
  return $LASTEXITCODE -eq 0
}

function Install-HostPythonDeps {
  if (-not (Get-Command python -ErrorAction SilentlyContinue)) {
    return $false
  }
  New-Item -ItemType Directory -Force -Path $PythonDepsDir | Out-Null
  if (Test-PythonDeps) {
    return $true
  }

  & python -m pip --version *> $null
  if ($LASTEXITCODE -eq 0) {
    & python -m pip install --quiet --target $PythonDepsDir -r benchmarks/requirements.txt
    if (Test-PythonDeps) {
      return $true
    }
  }

  & python -m ensurepip --user *> $null
  if ($LASTEXITCODE -eq 0) {
    & python -m pip install --quiet --target $PythonDepsDir -r benchmarks/requirements.txt
    if (Test-PythonDeps) {
      return $true
    }
  }

  return $false
}

function Invoke-HostPythonSuite {
  $Failed = $false
  $Scripts = @(
    "benchmarks/seed/seed_memories.py",
    "benchmarks/scripts/run_latency.py",
    "benchmarks/scripts/run_token_savings.py",
    "benchmarks/scripts/run_recall_quality.py",
    "benchmarks/scripts/run_temporal_correctness.py",
    "benchmarks/scripts/run_narrative_archival.py"
  )
  foreach ($Script in $Scripts) {
    & python $Script --results-dir $ResultsDir
    if ($LASTEXITCODE -ne 0) {
      $Failed = $true
    }
  }
  return -not $Failed
}

function Get-DockerResultsDir {
  $RootUnix = $Root.Replace("\", "/")
  $ResultsUnix = $ResultsDir.Replace("\", "/")
  if ($ResultsUnix.StartsWith($RootUnix)) {
    return "/repo" + $ResultsUnix.Substring($RootUnix.Length)
  }
  return $ResultsUnix
}

function Get-BenchmarkPythonImage {
  if ($env:BENCHMARK_PYTHON_IMAGE) {
    return $env:BENCHMARK_PYTHON_IMAGE
  }
  & docker image inspect python:3.12-slim *> $null
  if ($LASTEXITCODE -eq 0) {
    return "python:3.12-slim"
  }
  & docker image inspect python:3.11-slim *> $null
  if ($LASTEXITCODE -eq 0) {
    return "python:3.11-slim"
  }
  return "python:3.12-slim"
}

function Invoke-DockerPythonSuite {
  if (-not (Get-Command docker -ErrorAction SilentlyContinue)) {
    return $false
  }
  $DockerResultsDir = Get-DockerResultsDir
  $PythonImage = Get-BenchmarkPythonImage
  $DockerScript = @(
    "python -m pip install --quiet -r benchmarks/requirements.txt",
    "failed=0",
    "for script in \",
    "  benchmarks/seed/seed_memories.py \",
    "  benchmarks/scripts/run_latency.py \",
    "  benchmarks/scripts/run_token_savings.py \",
    "  benchmarks/scripts/run_recall_quality.py \",
    "  benchmarks/scripts/run_temporal_correctness.py \",
    "  benchmarks/scripts/run_narrative_archival.py",
    "do",
    '  python "$script" --results-dir "$BENCHMARK_RESULTS_DIR" || failed=1',
    "done",
    'exit "$failed"'
  ) -join "`n"
  $DockerArgs = @(
    "run", "--rm", "--network", "host",
    "-v", "${Root}:/repo",
    "-w", "/repo",
    "-e", "BENCHMARK_RESULTS_DIR=$DockerResultsDir",
    "-e", "AEON_BASE_URL=$env:AEON_BASE_URL",
    "-e", "MOCK_BASE_URL=$env:MOCK_BASE_URL",
    "-e", "DATABASE_URL=$env:DATABASE_URL",
    "-e", "MANAGEMENT_API_KEY=$env:MANAGEMENT_API_KEY",
    "-e", "MOCK_EMBEDDING_MODE=$env:MOCK_EMBEDDING_MODE",
    "-e", "MOCK_ARCHIVAL_COMPACTION=$env:MOCK_ARCHIVAL_COMPACTION",
    "-e", "ALLOW_UNAUTH_MANAGEMENT=$env:ALLOW_UNAUTH_MANAGEMENT",
    $PythonImage,
    "bash", "-lc", $DockerScript
  )
  & docker @DockerArgs
  return $LASTEXITCODE -eq 0
}

function Invoke-PythonSuite {
  if (Install-HostPythonDeps) {
    if (-not (Invoke-HostPythonSuite)) {
      Write-Host "one or more Python benchmark scripts failed; summary will mark missing or failed artifacts"
    }
    return
  }

  Write-Host "host Python benchmark dependencies unavailable; trying Docker Python runner"
  if (Invoke-DockerPythonSuite) {
    return
  }

  Write-Host "Docker Python runner failed or is unavailable; falling back to host Python best effort"
  if (-not (Invoke-HostPythonSuite)) {
    Write-Host "one or more Python benchmark scripts failed; summary will mark missing or failed artifacts"
  }
}

function Invoke-K6Suite {
  if (Get-Command k6 -ErrorAction SilentlyContinue) {
    & k6 run -e AEON_BASE_URL=$env:AEON_BASE_URL benchmarks/k6/proxy_latency.js --summary-export "$ResultsDir\k6_proxy_latency.json"
    & k6 run -e AEON_BASE_URL=$env:AEON_BASE_URL benchmarks/k6/retrieval_latency.js --summary-export "$ResultsDir\k6_retrieval_latency.json"
    return
  }

  if (Get-Command docker -ErrorAction SilentlyContinue) {
    $DockerResultsDir = Get-DockerResultsDir
    & docker run --rm --network host -v "${Root}:/repo" -w /repo -e AEON_BASE_URL=$env:AEON_BASE_URL grafana/k6 run benchmarks/k6/proxy_latency.js --summary-export "$DockerResultsDir/k6_proxy_latency.json"
    & docker run --rm --network host -v "${Root}:/repo" -w /repo -e AEON_BASE_URL=$env:AEON_BASE_URL grafana/k6 run benchmarks/k6/retrieval_latency.js --summary-export "$DockerResultsDir/k6_retrieval_latency.json"
  }

  if (-not (Test-Path "$ResultsDir\k6_proxy_latency.json") -and -not (Test-Path "$ResultsDir\k6_retrieval_latency.json")) {
    '{"status":"not_run","reason":"k6 not found and Docker k6 fallback did not produce artifacts"}' | Out-File -Encoding utf8 "$ResultsDir\k6.json"
  }
}

if (Get-Command docker -ErrorAction SilentlyContinue) {
  docker compose -f docker-compose.test.yml up --build -d
} else {
  Write-Host "docker not found; scripts will record service-level failures if AEON-IQ is not already running"
}

Invoke-PythonSuite
Invoke-K6Suite
python benchmarks/scripts/summarize_results.py --results-dir $ResultsDir

Write-Host "Benchmark summary: $ResultsDir\summary.json"
