#!/usr/bin/env python3
"""
Mock OpenAI-compatible server for AEON-IQ integration testing.
Handles /v1/embeddings and /v1/chat/completions.
Runs on port 11435 (configurable via PORT env var).
"""

import json
import math
import os
import sys
import time
import hashlib
import re
from http.server import BaseHTTPRequestHandler, HTTPServer
from urllib.parse import urlparse

PORT = int(os.environ.get("MOCK_PORT", 11435))
DIM  = int(os.environ.get("EMBEDDING_DIMENSION", 1536))
EMBEDDING_MODE = os.environ.get("MOCK_EMBEDDING_MODE", "constant").lower()
MOCK_ARCHIVAL_COMPACTION = os.environ.get("MOCK_ARCHIVAL_COMPACTION", "false").lower() in {
    "1",
    "true",
    "yes",
}

EXTRACTION_SIGNAL = "MemoryOS MMU"
WORKING_MEMORY_SIGNAL = "You are MemoryOS Working Memory"
COMPACTION_SIGNAL = "You are compressing"
TOKEN_RE = re.compile(r"[a-z0-9]+")
STOPWORDS = {
    "a", "an", "and", "are", "as", "at", "for", "from", "how", "i", "in",
    "is", "it", "me", "my", "of", "on", "or", "the", "this", "to", "what",
    "with", "you", "your",
}


def make_embedding(text: str, dim: int = DIM) -> list:
    """Return a constant unit-norm embedding (all texts get the same vector).
    This ensures cosine distance = 0 between all pairs, so every memory is
    retrieved regardless of semantic content — correct for integration testing.
    """
    if EMBEDDING_MODE == "hash":
        vec = [0.0] * dim
        tokens = [t for t in TOKEN_RE.findall(text.lower()) if t not in STOPWORDS]
        if not tokens:
            tokens = ["empty"]
        for token in tokens:
            digest = hashlib.sha256(token.encode()).digest()
            idx = int.from_bytes(digest[:4], "big") % dim
            vec[idx] += 1.0
        norm = math.sqrt(sum(v * v for v in vec)) or 1.0
        return [v / norm for v in vec]

    # All-ones vector normalized to unit norm
    val = 1.0 / math.sqrt(dim)
    return [val] * dim


def compaction_response(body: dict) -> dict:
    """Return deterministic archival compaction output for benchmark runs."""
    messages = body.get("messages", [])
    prompt = " ".join(
        m.get("content", "") for m in messages if isinstance(m.get("content"), str)
    )
    source_lines = [
        line.strip()
        for line in prompt.splitlines()
        if line.strip() and line.strip()[0].isdigit()
    ]
    facts = [
        "Benchmark archival sources describe Mira's Nimbus project preferences.",
        "Nimbus benchmark memories cover Rust services, weekly planning, and audit trails.",
        "The archived benchmark facts should remain linked to one reversible archival batch.",
    ]
    if source_lines:
        facts[0] = f"Benchmark archival compacted {len(source_lines)} stale source memories."

    content = json.dumps(
        {
            "facts": facts,
            "narrative": (
                "Mira's Nimbus work repeatedly emphasized Rust service design, "
                "weekly planning, and auditable memory history. The archived "
                "material forms one cohesive benchmark storyline."
            ),
        }
    )
    return {
        "id": "chatcmpl-mock-compaction",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": body.get("model", "gpt-4o-mini"),
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": content},
                "finish_reason": "stop",
            }
        ],
        "usage": {"prompt_tokens": 120, "completion_tokens": 90, "total_tokens": 210},
    }


def extraction_response(body: dict) -> dict:
    """Return a structured extraction JSON for MemoryOS MMU calls."""
    # Detect the user message content to tailor extraction
    messages = body.get("messages", [])
    transcript = " ".join(m.get("content", "") for m in messages if isinstance(m.get("content"), str))

    facts = []
    entities = []
    relations = []

    if "Cael" in transcript and "AEON-IQ" in transcript:
        facts = [
            {
                "content": "User's name is Cael (cited: line 1)",
                "provenance": "user_stated",
                "cited_line": 1,
                "confidence": 0.97,
                "importance_score": 1.0,
                "importance_source": "extractor",
            },
            {
                "content": "Cael is building AEON-IQ (cited: line 1)",
                "provenance": "user_stated",
                "cited_line": 1,
                "confidence": 0.97,
                "importance_score": 0.95,
                "importance_source": "extractor",
            },
        ]
        entities = [
            {"name": "Cael", "type": "person", "confidence": 0.99},
            {"name": "AEON-IQ", "type": "product", "confidence": 0.99},
        ]
        relations = [
            {"subject": "Cael", "predicate": "is_building", "object": "AEON-IQ"}
        ]
        summary = "Cael is the user. They are building AEON-IQ."
        active_entities = ["Cael", "AEON-IQ"]
        current_goal = "Build AEON-IQ"
    elif "ALLabs" in transcript:
        facts = [
            {
                "content": "Cael's company is ALLabs (cited: line 1)",
                "provenance": "user_stated",
                "cited_line": 1,
                "confidence": 0.97,
                "importance_score": 0.9,
                "importance_source": "extractor",
            }
        ]
        entities = [{"name": "ALLabs", "type": "company", "confidence": 0.99}]
        relations = [{"subject": "Cael", "predicate": "works_at", "object": "ALLabs"}]
        summary = "Cael's company is ALLabs."
        active_entities = ["ALLabs"]
        current_goal = "Update company info"
    else:
        facts = []
        entities = []
        relations = []
        summary = "No significant facts extracted."
        active_entities = []
        current_goal = ""

    extraction_json = json.dumps({
        "facts": facts,
        "entities": entities,
        "relations": relations,
        "updated_summary": summary,
        "active_entities": active_entities,
        "current_goal": current_goal,
        "open_questions": [],
        "memory_type": "semantic",
        "confidence_low": False,
    })

    return {
        "id": "chatcmpl-mock-extraction",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": "gpt-4o-mini",
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": extraction_json},
                "finish_reason": "stop",
            }
        ],
        "usage": {"prompt_tokens": 100, "completion_tokens": 50, "total_tokens": 150},
    }


def working_memory_response(body: dict) -> dict:
    """Return a working memory update response."""
    messages = body.get("messages", [])
    transcript = " ".join(m.get("content", "") for m in messages if isinstance(m.get("content"), str))
    if "Cael" in transcript and "AEON-IQ" in transcript:
        summary = "Cael is building AEON-IQ."
    else:
        summary = "Ongoing conversation."

    wm_json = json.dumps({
        "summary": summary,
        "active_entities": ["Cael", "AEON-IQ"],
        "current_goal": "Build AEON-IQ",
        "open_questions": [],
    })

    return {
        "id": "chatcmpl-mock-wm",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": "gpt-4o-mini",
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": wm_json},
                "finish_reason": "stop",
            }
        ],
        "usage": {"prompt_tokens": 50, "completion_tokens": 30, "total_tokens": 80},
    }


def chat_response(body: dict) -> dict:
    """Return a normal chat completion, referencing injected memories if present."""
    messages = body.get("messages", [])
    stream = body.get("stream", False)

    # Collect all content for analysis
    all_content = " ".join(
        m.get("content", "") for m in messages if isinstance(m.get("content"), str)
    )

    # Check if memories were injected (tag has attributes, so match prefix only)
    has_memories = "<retrieved_memories" in all_content
    user_msg = ""
    for m in reversed(messages):
        if m.get("role") == "user":
            user_msg = m.get("content", "")
            break

    # Build reply
    if "What do you know about me" in user_msg or "what do you know about me" in user_msg:
        if has_memories:
            reply = (
                "Based on what I remember: Your name is Cael and you are building AEON-IQ. "
                "You are the founder of this project."
            )
        else:
            reply = "I don't have any prior information about you."
    elif "Remember this" in user_msg or "My name is Cael" in user_msg:
        reply = "Got it! I'll remember that your name is Cael and that you're building AEON-IQ."
    else:
        reply = "I understand. How can I help you further?"

    if stream:
        # Return SSE chunks
        chunks = []
        words = reply.split()
        first_chunk = {
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {"role": "assistant", "content": ""}, "finish_reason": None}],
        }
        chunks.append(f"data: {json.dumps(first_chunk)}\n\n")
        for word in words:
            chunk = {
                "id": "chatcmpl-mock",
                "object": "chat.completion.chunk",
                "created": int(time.time()),
                "model": "gpt-4o-mini",
                "choices": [{"index": 0, "delta": {"content": word + " "}, "finish_reason": None}],
            }
            chunks.append(f"data: {json.dumps(chunk)}\n\n")
        final_chunk = {
            "id": "chatcmpl-mock",
            "object": "chat.completion.chunk",
            "created": int(time.time()),
            "model": "gpt-4o-mini",
            "choices": [{"index": 0, "delta": {}, "finish_reason": "stop"}],
        }
        chunks.append(f"data: {json.dumps(final_chunk)}\n\n")
        chunks.append("data: [DONE]\n\n")
        return "".join(chunks)

    return {
        "id": "chatcmpl-mock",
        "object": "chat.completion",
        "created": int(time.time()),
        "model": "gpt-4o-mini",
        "choices": [
            {
                "index": 0,
                "message": {"role": "assistant", "content": reply},
                "finish_reason": "stop",
            }
        ],
        "usage": {"prompt_tokens": 80, "completion_tokens": 40, "total_tokens": 120},
    }


class MockHandler(BaseHTTPRequestHandler):
    def log_message(self, format, *args):
        print(f"[MOCK] {self.address_string()} - {format % args}", flush=True)

    def send_json(self, data, status=200):
        body = json.dumps(data).encode()
        self.send_response(status)
        self.send_header("Content-Type", "application/json")
        self.send_header("Content-Length", str(len(body)))
        self.end_headers()
        self.wfile.write(body)

    def send_sse(self, data: str):
        body = data.encode()
        self.send_response(200)
        self.send_header("Content-Type", "text/event-stream")
        self.send_header("Cache-Control", "no-cache")
        self.send_header("Transfer-Encoding", "chunked")
        self.end_headers()
        self.wfile.write(body)

    def do_POST(self):
        path = urlparse(self.path).path
        length = int(self.headers.get("Content-Length", 0))
        raw = self.rfile.read(length) if length > 0 else b"{}"
        try:
            body = json.loads(raw)
        except Exception:
            body = {}

        if path == "/v1/embeddings":
            inputs = body.get("input", [])
            if isinstance(inputs, str):
                inputs = [inputs]
            embeddings = [
                {"object": "embedding", "embedding": make_embedding(t), "index": i}
                for i, t in enumerate(inputs)
            ]
            self.send_json({
                "object": "list",
                "data": embeddings,
                "model": body.get("model", "text-embedding-3-small"),
                "usage": {"prompt_tokens": len(inputs) * 5, "total_tokens": len(inputs) * 5},
            })

        elif path == "/v1/chat/completions":
            messages = body.get("messages", [])
            all_text = " ".join(
                m.get("content", "") for m in messages if isinstance(m.get("content"), str)
            )

            if MOCK_ARCHIVAL_COMPACTION and COMPACTION_SIGNAL in all_text:
                self.send_json(compaction_response(body))
            elif EXTRACTION_SIGNAL in all_text:
                self.send_json(extraction_response(body))
            elif WORKING_MEMORY_SIGNAL in all_text:
                self.send_json(working_memory_response(body))
            elif body.get("stream", False):
                sse = chat_response(body)
                self.send_sse(sse)
            else:
                self.send_json(chat_response(body))

        else:
            self.send_json({"error": f"unknown path {path}"}, 404)

    def do_GET(self):
        path = urlparse(self.path).path
        if path == "/health":
            self.send_json({"status": "ok"})
        else:
            self.send_json({"error": "not found"}, 404)


if __name__ == "__main__":
    server = HTTPServer(("0.0.0.0", PORT), MockHandler)
    print(f"[MOCK] OpenAI-compatible mock server listening on port {PORT}", flush=True)
    try:
        server.serve_forever()
    except KeyboardInterrupt:
        print("[MOCK] Shutting down", flush=True)
        sys.exit(0)
