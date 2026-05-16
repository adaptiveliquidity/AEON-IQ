use axum::{body::Body, extract::State, http::StatusCode, response::Response};
use prometheus::{
    Counter, CounterVec, Encoder, Histogram, HistogramOpts, HistogramVec, Opts, Registry,
    TextEncoder,
};
use std::sync::Arc;

// ── Metric definitions ────────────────────────────────────────────────────────

#[derive(Clone)]
pub struct Metrics {
    pub registry: Arc<Registry>,

    /// Total HTTP requests, labelled by coarsened path and HTTP status code.
    pub requests_total: CounterVec,

    /// End-to-end handler latency in seconds, labelled by path.
    pub request_duration: HistogramVec,

    /// Memory injection outcomes: `result = hit | miss`.
    pub injection_total: CounterVec,

    /// How many memories were injected on a hit (0 on a miss).
    pub injected_per_req: Histogram,

    /// MMU background extraction outcomes: `status = ok | error | low_confidence`.
    pub extraction_total: CounterVec,

    /// End-to-end extraction LLM call latency.
    pub extraction_secs: Histogram,

    /// pgvector HNSW similarity search latency.
    pub vector_search_secs: Histogram,

    /// LTM archival cycles: `status = ok | error`.
    pub archival_total: CounterVec,

    /// Number of memories compacted per archival cycle.
    pub archival_compacted: Histogram,

    /// Requests rejected by the per-agent rate limiter.
    pub rate_limited_total: Counter,
}

impl Metrics {
    pub fn new() -> Result<Self, prometheus::Error> {
        let reg = Registry::new();

        let req_buckets  = vec![0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5, 1.0, 2.5, 5.0];
        let fast_buckets = vec![0.001, 0.002, 0.005, 0.01, 0.025, 0.05, 0.1, 0.25, 0.5];
        let count_buckets= vec![0.0, 1.0, 2.0, 3.0, 4.0, 5.0, 10.0, 25.0, 50.0];

        macro_rules! reg {
            ($m:expr) => {{
                let m = $m;
                reg.register(Box::new(m.clone()))?;
                m
            }};
        }

        let requests_total = reg!(CounterVec::new(
            Opts::new("memoryos_requests_total", "Total HTTP requests"),
            &["path", "status"],
        )?);

        let request_duration = reg!(HistogramVec::new(
            HistogramOpts::new(
                "memoryos_request_duration_seconds",
                "HTTP request latency (seconds)",
            )
            .buckets(req_buckets.clone()),
            &["path"],
        )?);

        let injection_total = reg!(CounterVec::new(
            Opts::new(
                "memoryos_injection_total",
                "Memory injection outcomes (hit = memories found, miss = none)",
            ),
            &["result"],
        )?);

        let injected_per_req = reg!(Histogram::with_opts(
            HistogramOpts::new(
                "memoryos_injected_count",
                "Number of memories injected per request (on a hit)",
            )
            .buckets(count_buckets.clone()),
        )?);

        let extraction_total = reg!(CounterVec::new(
            Opts::new("memoryos_extraction_total", "MMU background extraction outcomes"),
            &["status"],
        )?);

        let extraction_secs = reg!(Histogram::with_opts(
            HistogramOpts::new(
                "memoryos_extraction_duration_seconds",
                "Time for the extraction LLM call (seconds)",
            )
            .buckets(req_buckets),
        )?);

        let vector_search_secs = reg!(Histogram::with_opts(
            HistogramOpts::new(
                "memoryos_vector_search_duration_seconds",
                "pgvector HNSW search latency (seconds)",
            )
            .buckets(fast_buckets),
        )?);

        let archival_total = reg!(CounterVec::new(
            Opts::new("memoryos_archival_total", "LTM archival cycle outcomes"),
            &["status"],
        )?);

        let archival_compacted = reg!(Histogram::with_opts(
            HistogramOpts::new(
                "memoryos_archival_compacted",
                "L2 memories compacted per archival cycle",
            )
            .buckets(count_buckets),
        )?);

        let rate_limited_total = reg!(Counter::with_opts(Opts::new(
            "memoryos_rate_limited_total",
            "Requests rejected by the per-agent rate limiter",
        ))?);

        Ok(Self {
            registry: Arc::new(reg),
            requests_total,
            request_duration,
            injection_total,
            injected_per_req,
            extraction_total,
            extraction_secs,
            vector_search_secs,
            archival_total,
            archival_compacted,
            rate_limited_total,
        })
    }

    /// Render all metrics in Prometheus text format 0.0.4.
    pub fn render(&self) -> String {
        let encoder = TextEncoder::new();
        let mf = self.registry.gather();
        let mut buf = Vec::new();
        encoder.encode(&mf, &mut buf).ok();
        String::from_utf8(buf).unwrap_or_default()
    }
}

// ── /metrics handler ──────────────────────────────────────────────────────────

pub async fn handle_metrics(State(state): State<crate::AppState>) -> Response<Body> {
    Response::builder()
        .status(StatusCode::OK)
        .header(
            "content-type",
            "text/plain; version=0.0.4; charset=utf-8",
        )
        .body(Body::from(state.metrics.render()))
        .unwrap()
}

// ── Request-tracking middleware ───────────────────────────────────────────────

pub async fn track_request(
    State(state): State<crate::AppState>,
    req: axum::extract::Request,
    next: axum::middleware::Next,
) -> axum::response::Response {
    let path = coarsen_path(req.uri().path()).to_string();
    let start = std::time::Instant::now();

    let response = next.run(req).await;

    let elapsed = start.elapsed().as_secs_f64();
    let status = response.status().as_u16().to_string();

    state
        .metrics
        .requests_total
        .with_label_values(&[&path, &status])
        .inc();
    state
        .metrics
        .request_duration
        .with_label_values(&[&path])
        .observe(elapsed);

    response
}

/// Replace opaque path segments with a fixed placeholder so that label
/// cardinality stays bounded regardless of how many unique agent IDs exist.
fn coarsen_path(path: &str) -> &str {
    match path {
        "/v1/chat/completions"     => "/v1/chat/completions",
        "/v1/models"               => "/v1/models",
        "/api/v1/agents"           => "/api/v1/agents",
        "/api/v1/stats"            => "/api/v1/stats",
        "/api/v1/memories/search"  => "/api/v1/memories/search",
        "/metrics"                 => "/metrics",
        "/health"                  => "/health",
        p if p.ends_with("/memories") && p.starts_with("/api/v1/agents/") => {
            "/api/v1/agents/:id/memories"
        }
        p if p.starts_with("/api/v1/memories/") => "/api/v1/memories/:id",
        _ => "/other",
    }
}
