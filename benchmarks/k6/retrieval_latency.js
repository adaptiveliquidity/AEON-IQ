import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate } from "k6/metrics";

export const options = {
  vus: Number(__ENV.K6_VUS || 2),
  duration: __ENV.K6_DURATION || "30s",
  thresholds: {
    http_req_failed: ["rate<0.10"],
    http_req_duration: ["p(95)<2000"],
    semantic_search_duration: ["p(95)<2000"],
    temporal_at_duration: ["p(95)<2000"],
    temporal_diff_duration: ["p(95)<2000"],
    retrieval_logs_duration: ["p(95)<2000"],
  },
};

const base = __ENV.AEON_BASE_URL || "http://localhost:8080";
const searchAgent = __ENV.K6_SEARCH_AGENT_ID || "bench-retrieval-1000";
const temporalAgent = __ENV.K6_TEMPORAL_AGENT_ID || "bench-temporal";

const semanticSearchDuration = new Trend("semantic_search_duration");
const temporalAtDuration = new Trend("temporal_at_duration");
const temporalDiffDuration = new Trend("temporal_diff_duration");
const retrievalLogsDuration = new Trend("retrieval_logs_duration");
const endpointFailed = new Rate("retrieval_endpoint_failed");

function authHeaders() {
  const headers = { "Content-Type": "application/json" };
  if (__ENV.MANAGEMENT_API_KEY) {
    headers["X-Management-Key"] = __ENV.MANAGEMENT_API_KEY;
  }
  return headers;
}

export default function () {
  const headers = authHeaders();
  const searchBody = JSON.stringify({
    agent_id: searchAgent,
    query: `Nimbus vector probe topic ${__ITER % 25}`,
    limit: 5,
    threshold: 0.95,
  });
  let res = http.post(`${base}/api/v1/memories/search`, searchBody, { headers, timeout: "30s" });
  semanticSearchDuration.add(res.timings.duration);
  endpointFailed.add(res.status < 200 || res.status >= 300);
  check(res, { "search status is 2xx": (r) => r.status >= 200 && r.status < 300 });

  const now = new Date().toISOString();
  res = http.get(
    `${base}/api/v1/agents/${temporalAgent}/memories/at?timestamp=${encodeURIComponent(now)}&limit=20&offset=0`,
    { headers, timeout: "30s" },
  );
  temporalAtDuration.add(res.timings.duration);
  endpointFailed.add(res.status < 200 || res.status >= 300);
  check(res, { "memories/at status is 2xx": (r) => r.status >= 200 && r.status < 300 });

  const earlier = new Date(Date.now() - 60 * 60 * 1000).toISOString();
  res = http.get(
    `${base}/api/v1/agents/${temporalAgent}/memories/diff?from=${encodeURIComponent(earlier)}&to=${encodeURIComponent(now)}`,
    { headers, timeout: "30s" },
  );
  temporalDiffDuration.add(res.timings.duration);
  endpointFailed.add(res.status < 200 || res.status >= 300);
  check(res, { "memories/diff status is 2xx": (r) => r.status >= 200 && r.status < 300 });

  res = http.get(`${base}/api/v1/agents/bench-recall/retrievals?limit=20`, { headers, timeout: "30s" });
  retrievalLogsDuration.add(res.timings.duration);
  endpointFailed.add(res.status < 200 || res.status >= 300);
  check(res, { "retrievals status is 2xx": (r) => r.status >= 200 && r.status < 300 });

  sleep(0.2);
}
