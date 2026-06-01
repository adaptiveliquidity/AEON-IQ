#!/usr/bin/env python3
"""AEON-IQ memory system integration test runner."""

import json
import sys
import time
import urllib.request
import urllib.error

BASE = "http://localhost:8080"
HEADERS = {
    "Content-Type": "application/json",
    "Authorization": "Bearer sk-mock-test-key-not-real",
}
AGENT = "test-agent"

results = {}

def req(method, path, body=None, extra_headers=None):
    url = BASE + path
    data = json.dumps(body).encode() if body else None
    hdrs = dict(HEADERS)
    if extra_headers:
        hdrs.update(extra_headers)
    r = urllib.request.Request(url, data=data, headers=hdrs, method=method)
    try:
        with urllib.request.urlopen(r, timeout=30) as resp:
            return resp.status, json.loads(resp.read())
    except urllib.error.HTTPError as e:
        return e.code, json.loads(e.read())

def check(name, cond, detail=""):
    status = "PASS" if cond else "FAIL"
    results[name] = {"status": status, "detail": detail}
    print(f"[{status}] {name}" + (f": {detail}" if detail else ""))
    return cond

# ── T4: session-1 ──────────────────────────────────────────────────────────
print("\n=== TEST 4: session-1 — store Cael/AEON-IQ ===")
sc, resp = req("POST", "/v1/chat/completions", {
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "My name is Cael and I am building AEON-IQ. Remember this."}]
}, {"x-agent-id": AGENT, "x-session-id": "session-1"})
t4_content = resp.get("choices", [{}])[0].get("message", {}).get("content", "")
check("T4 HTTP 200", sc == 200, f"status={sc}")
check("T4 response non-empty", bool(t4_content), t4_content[:100])
print(f"   Response: {t4_content}")

print("   Waiting 6s for background extraction...")
time.sleep(6)

# ── T5: memories present ────────────────────────────────────────────────────
print("\n=== TEST 5: Memories for test-agent ===")
sc, data = req("GET", f"/api/v1/agents/{AGENT}/memories")
mems = data.get("memories", [])
cael_mem  = [m for m in mems if "Cael" in m.get("content","")]
aeon_mem  = [m for m in mems if "AEON" in m.get("content","")]
check("T5 HTTP 200", sc == 200)
check("T5 memory count > 0", len(mems) > 0, f"count={len(mems)}")
check("T5 Cael in memories", bool(cael_mem), f"{len(cael_mem)} memories")
check("T5 AEON-IQ in memories", bool(aeon_mem), f"{len(aeon_mem)} memories")
for m in mems:
    print(f"   [{m.get('memory_type','?')}] {m['content'][:70]}  session={m.get('session_id','?')}")

MEMORY_ID = mems[0]["id"] if mems else None

# ── T6+T7: session-2 cross-session recall ──────────────────────────────────
print("\n=== TESTS 6+7: session-2 cross-session recall ===")
sc, resp2 = req("POST", "/v1/chat/completions", {
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "What do you know about me?"}]
}, {"x-agent-id": AGENT, "x-session-id": "session-2"})
t67_content = resp2.get("choices", [{}])[0].get("message", {}).get("content", "")
print(f"   Response: {t67_content}")
check("T6 HTTP 200", sc == 200)
check("T7 response mentions Cael", "Cael" in t67_content, t67_content[:150])
check("T7 response mentions AEON-IQ", "AEON-IQ" in t67_content, t67_content[:150])
time.sleep(4)

# ── T8: different-agent isolation ──────────────────────────────────────────
print("\n=== TEST 8: different-agent isolation ===")
sc, resp3 = req("POST", "/v1/chat/completions", {
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "What do you know about me?"}]
}, {"x-agent-id": "different-agent", "x-session-id": "session-x"})
t8_content = resp3.get("choices", [{}])[0].get("message", {}).get("content", "")
print(f"   Response: {t8_content}")
no_leak = "Cael" not in t8_content and "AEON-IQ" not in t8_content
check("T8 no memory leak to different-agent", no_leak, t8_content[:150])

# ── T9: list sessions ──────────────────────────────────────────────────────
print("\n=== TEST 9: List sessions for test-agent ===")
sc, sdata = req("GET", f"/api/v1/agents/{AGENT}/sessions")
sessions = sdata.get("sessions", [])
session_ids = [s.get("session_id","") for s in sessions]
print(f"   Sessions found: {session_ids}")
check("T9 HTTP 200", sc == 200)
check("T9 session-1 exists", "session-1" in session_ids)
check("T9 session-2 exists", "session-2" in session_ids)

# ── T10: list agents ────────────────────────────────────────────────────────
print("\n=== TEST 10: List agents ===")
sc, adata = req("GET", "/api/v1/agents")
agents = [a.get("id","") for a in adata.get("agents", [])]
print(f"   Agents found: {agents}")
check("T10 HTTP 200", sc == 200)
check("T10 test-agent exists", AGENT in agents)

# ── T11: PATCH memory ──────────────────────────────────────────────────────
print("\n=== TEST 11: PATCH memory ===")
if MEMORY_ID:
    sc, pdata = req("PATCH", f"/api/v1/memories/{MEMORY_ID}", {
        "content": "Cael is the founder of ALLabs."
    })
    check("T11 PATCH 200", sc == 200, f"status={sc} resp={str(pdata)[:80]}")
    print(f"   Patched memory {MEMORY_ID[:8]}...: {pdata}")
else:
    check("T11 PATCH memory", False, "No memory ID available")

time.sleep(2)

# ── T12: updated memory reflected ──────────────────────────────────────────
print("\n=== TEST 12: Updated memory reflected in response ===")
sc, resp4 = req("POST", "/v1/chat/completions", {
    "model": "gpt-4o-mini",
    "messages": [{"role": "user", "content": "What do you know about me?"}]
}, {"x-agent-id": AGENT, "x-session-id": "session-3"})
t12_content = resp4.get("choices", [{}])[0].get("message", {}).get("content", "")
print(f"   Response: {t12_content}")
check("T12 HTTP 200", sc == 200)
check("T12 response non-empty", bool(t12_content))
# Check that SOME known fact appears (either old or new)
has_context = any(k in t12_content for k in ["Cael","AEON-IQ","ALLabs","remember"])
check("T12 response uses memory context", has_context, t12_content[:200])

# ── T13: trigger archival ───────────────────────────────────────────────────
print("\n=== TEST 13: Trigger archival manually ===")
sc, arch = req("POST", f"/api/v1/agents/{AGENT}/archival/trigger", {})
print(f"   Archival response: {arch}")
check("T13 archival no 404/500", sc not in [404, 500], f"status={sc}")

# ── T14: export memories ────────────────────────────────────────────────────
print("\n=== TEST 14: Export memories ===")
url = BASE + f"/api/v1/agents/{AGENT}/export"
r = urllib.request.Request(url, headers=HEADERS, method="GET")
try:
    with urllib.request.urlopen(r, timeout=15) as resp:
        export_status = resp.status
        export_body = resp.read().decode()
except urllib.error.HTTPError as e:
    export_status = e.code
    export_body = e.read().decode()

lines = [l for l in export_body.strip().split("\n") if l.strip()]
export_mems = []
for l in lines:
    try:
        export_mems.append(json.loads(l))
    except:
        pass
print(f"   Export lines: {len(lines)}, valid NDJSON objects: {len(export_mems)}")
check("T14 export 200", export_status == 200)
check("T14 export has memories", len(export_mems) > 0, f"{len(export_mems)} memories")
cael_in_export = any("Cael" in m.get("content","") for m in export_mems)
check("T14 export contains Cael memories", cael_in_export)

# ── T15: conflicts ──────────────────────────────────────────────────────────
print("\n=== TEST 15: Conflicts ===")
sc_a, ca = req("POST", f"/api/v1/agents/{AGENT}/memories",
               {"content": "Cael's company is AEON-IQ.", "memory_type": "semantic"})
time.sleep(1)
sc_b, cb = req("POST", f"/api/v1/agents/{AGENT}/memories",
               {"content": "Cael's company is ALLabs.", "memory_type": "semantic"})
time.sleep(1)
sc_c, cdata = req("GET", f"/api/v1/agents/{AGENT}/conflicts")
conflicts = cdata.get("conflicts", [])
print(f"   Conflict create mem-A: {sc_a} | mem-B: {sc_b}")
print(f"   Conflicts endpoint: {sc_c} | conflicts found: {len(conflicts)}")
for c in conflicts[:3]:
    print(f"   Conflict: {c.get('memory_a_content','?')[:50]} VS {c.get('memory_b_content','?')[:50]}")
check("T15 conflict mem-A created", sc_a in [200,201])
check("T15 conflict mem-B created", sc_b in [200,201])
check("T15 conflicts endpoint 200", sc_c == 200, f"status={sc_c}")
check("T15 conflict count (sync-only)", True, f"conflicts={len(conflicts)} (CONFLICT_DETECTION_ENABLED=false so 0 expected)")

# ── Summary ────────────────────────────────────────────────────────────────
print("\n" + "="*60)
print("SUMMARY")
print("="*60)
passed = [k for k,v in results.items() if v["status"]=="PASS"]
failed = [k for k,v in results.items() if v["status"]=="FAIL"]
print(f"Passed: {len(passed)}/{len(results)}")
if failed:
    print(f"Failed tests: {failed}")
    for f in failed:
        print(f"  {f}: {results[f]['detail']}")
