import http from "k6/http";
import { check, sleep } from "k6";
import { Trend, Rate } from "k6/metrics";

export const options = {
  vus: Number(__ENV.K6_VUS || 4),
  duration: __ENV.K6_DURATION || "30s",
  thresholds: {
    http_req_failed: ["rate<0.05"],
    http_req_duration: ["p(95)<2000"],
    proxy_chat_duration: ["p(95)<2000"],
  },
};

const base = __ENV.AEON_BASE_URL || "http://localhost:8080";
const agentId = __ENV.K6_AGENT_ID || "bench-latency-seeded";
const proxyChatDuration = new Trend("proxy_chat_duration");
const proxyChatFailed = new Rate("proxy_chat_failed");

export default function () {
  const body = JSON.stringify({
    model: "gpt-4o-mini",
    messages: [{ role: "user", content: `K6 latency probe topic ${__ITER % 25}` }],
    stream: false,
  });
  const params = {
    headers: {
      "Content-Type": "application/json",
      "x-agent-id": agentId,
      "x-session-id": `k6-proxy-${__VU}-${__ITER}`,
    },
    timeout: "30s",
  };
  const res = http.post(`${base}/v1/chat/completions`, body, params);
  proxyChatDuration.add(res.timings.duration);
  proxyChatFailed.add(res.status < 200 || res.status >= 300);
  check(res, {
    "chat status is 2xx": (r) => r.status >= 200 && r.status < 300,
    "chat body has choices": (r) => (r.json("choices") || []).length >= 1,
  });
  sleep(0.1);
}
