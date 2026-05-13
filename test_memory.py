#!/usr/bin/env python3
"""
MemoryOS Kernel – Memory Retention Test
========================================
Sends a 20-turn conversation through the kernel.
Turn 1 introduces a name + startup.
Turns 2-19 are completely unrelated technical questions.
Turn 20 asks the agent to recall the facts from turn 1.

Pass criterion: the response contains both "alex" and "novapay" (case-insensitive).

Usage:
    OPENAI_API_KEY=sk-... python test_memory.py

    # Custom kernel URL:
    KERNEL_URL=http://localhost:8080 OPENAI_API_KEY=sk-... python test_memory.py
"""

import os
import sys
import time

try:
    from openai import OpenAI
except ImportError:
    print("ERROR: openai package not found.  Run: pip install openai")
    sys.exit(1)

KERNEL_URL   = os.environ.get("KERNEL_URL", "http://localhost:8080")
OPENAI_KEY   = os.environ.get("OPENAI_API_KEY", "sk-test")
AGENT_ID     = "memory-test-agent"
MODEL        = os.environ.get("TEST_MODEL", "gpt-4o-mini")
EXTRACT_WAIT = int(os.environ.get("EXTRACT_WAIT", "4"))  # seconds to wait for background MMU

DISTRACTOR_QUESTIONS = [
    "What is the capital of France?",
    "Explain Python decorators in one sentence.",
    "What is the speed of light in a vacuum?",
    "How does HTTPS differ from HTTP?",
    "What is a binary search tree?",
    "Explain REST vs GraphQL briefly.",
    "What are microservices?",
    "How does DNS resolution work?",
    "What is Kubernetes used for?",
    "What is the difference between TCP and UDP?",
    "Explain the CAP theorem in one line.",
    "What is a webhook?",
    "How does OAuth2 work at a high level?",
    "What is a CDN and why is it used?",
    "Explain ACID properties in databases.",
    "What is eventual consistency?",
    "How does Redis differ from PostgreSQL?",
    "What is a load balancer?",
]

def hr(char="─", width=60):
    print(char * width)

def main():
    client = OpenAI(
        api_key=OPENAI_KEY,
        base_url=f"{KERNEL_URL}/v1",
        default_headers={"x-agent-id": AGENT_ID},
    )

    hr("═")
    print("  MemoryOS Kernel — Memory Retention Test")
    hr("═")
    print(f"  Kernel  : {KERNEL_URL}")
    print(f"  Agent ID: {AGENT_ID}")
    print(f"  Model   : {MODEL}")
    hr()

    # ── Turn 1: introduce facts ────────────────────────────────────────────────
    print("\n[Turn 1] Introducing persona facts...")
    r1 = client.chat.completions.create(
        model=MODEL,
        messages=[{
            "role": "user",
            "content": (
                "My name is Alex and I'm building a fintech startup called NovaPay. "
                "We are working on instant cross-border payments. Please remember all of this."
            ),
        }],
    )
    print(f"  Agent: {r1.choices[0].message.content[:200]}")

    print(f"\n  [Waiting {EXTRACT_WAIT}s for background MMU extraction...]")
    time.sleep(EXTRACT_WAIT)

    # ── Turns 2-19: distractors ───────────────────────────────────────────────
    for i, question in enumerate(DISTRACTOR_QUESTIONS, start=2):
        print(f"\n[Turn {i}] {question[:60]}...")
        r = client.chat.completions.create(
            model=MODEL,
            messages=[{"role": "user", "content": question}],
        )
        snippet = r.choices[0].message.content[:80].replace("\n", " ")
        print(f"  Agent: {snippet}...")

    # ── Turn 20: recall test ──────────────────────────────────────────────────
    hr()
    print("\n[Turn 20] Recall test — what does the agent remember?")
    r_final = client.chat.completions.create(
        model=MODEL,
        messages=[{
            "role": "user",
            "content": (
                "Without looking anything up: what is my name, what startup am I building, "
                "and what does it do? Be specific."
            ),
        }],
    )
    response = r_final.choices[0].message.content
    print(f"\n  Agent response:\n{response}")

    # ── Verdict ───────────────────────────────────────────────────────────────
    hr("═")
    low = response.lower()
    found_name    = "alex"     in low
    found_startup = "novapay"  in low

    if found_name and found_startup:
        print("  ✅  PASS  — agent recalled 'Alex' and 'NovaPay' correctly")
        sys.exit(0)
    else:
        missing = []
        if not found_name:    missing.append("'Alex'")
        if not found_startup: missing.append("'NovaPay'")
        print(f"  ❌  FAIL  — missing: {', '.join(missing)}")
        print("  Tip: make sure the kernel is running and OPENAI_API_KEY is valid.")
        print("       The MMU extraction runs asynchronously — try increasing EXTRACT_WAIT.")
        sys.exit(1)

if __name__ == "__main__":
    main()
