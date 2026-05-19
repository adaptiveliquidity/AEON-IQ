-- MemoryOS Kernel – initial schema
-- Requires: PostgreSQL 14+ with pgvector extension
-- Embedding dimension defaults to 1536 (OpenAI text-embedding-3-small).
-- To use bge-small (384-dim) change the vector() sizes and EMBEDDING_DIMENSION env var.

CREATE EXTENSION IF NOT EXISTS vector;
CREATE EXTENSION IF NOT EXISTS "uuid-ossp";

-- ── Agents ────────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS agents (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   TEXT        UNIQUE NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata   JSONB       NOT NULL DEFAULT '{}'
);

-- ── Sessions ──────────────────────────────────────────────────────────────────
-- NOTE: sessions table is scaffolded; wired in Phase 1 (working memory API).
CREATE TABLE IF NOT EXISTS sessions (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    session_id TEXT        NOT NULL,
    agent_id   TEXT        NOT NULL REFERENCES agents(agent_id) ON DELETE CASCADE,
    started_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    ended_at   TIMESTAMPTZ,
    turn_count INTEGER     NOT NULL DEFAULT 0
);
CREATE UNIQUE INDEX IF NOT EXISTS idx_sessions_agent_session
    ON sessions(agent_id, session_id);

-- ── L2/L3 Memories ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS memories (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    TEXT        NOT NULL,
    session_id  TEXT,
    content     TEXT        NOT NULL,
    memory_type TEXT        NOT NULL DEFAULT 'episodic',  -- episodic | semantic | procedural
    confidence  FLOAT       NOT NULL DEFAULT 1.0,
    embedding   vector(1536),                              -- change to vector(384) for bge-small
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    source_turn INTEGER,
    citations   JSONB       NOT NULL DEFAULT '[]'
);

CREATE INDEX IF NOT EXISTS idx_memories_agent_id  ON memories(agent_id);
CREATE INDEX IF NOT EXISTS idx_memories_created   ON memories(created_at DESC);

-- HNSW index for sub-millisecond ANN search (pgvector >= 0.5)
CREATE INDEX IF NOT EXISTS idx_memories_hnsw
    ON memories USING hnsw (embedding vector_cosine_ops)
    WITH (m = 16, ef_construction = 64);

-- ── L1 Working Memory (per session) ──────────────────────────────────────────
CREATE TABLE IF NOT EXISTS working_memory (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   TEXT        NOT NULL,
    session_id TEXT        NOT NULL,
    summary    TEXT,
    turn_count INTEGER     NOT NULL DEFAULT 0,
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    UNIQUE(agent_id, session_id)
);

-- ── Entity graph ──────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS entities (
    id          UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id    TEXT        NOT NULL,
    name        TEXT        NOT NULL,
    entity_type TEXT        NOT NULL,
    confidence  FLOAT       NOT NULL DEFAULT 0.9,
    created_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at  TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    metadata    JSONB       NOT NULL DEFAULT '{}',
    UNIQUE(agent_id, name)
);
CREATE INDEX IF NOT EXISTS idx_entities_agent ON entities(agent_id);

-- ── Relation graph ────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS memory_graph (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   TEXT        NOT NULL,
    subject    TEXT        NOT NULL,
    predicate  TEXT        NOT NULL,
    object     TEXT        NOT NULL,
    confidence FLOAT       NOT NULL DEFAULT 0.9,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_graph_agent   ON memory_graph(agent_id);
CREATE INDEX IF NOT EXISTS idx_graph_subject ON memory_graph(agent_id, subject);

-- ── Audit log ─────────────────────────────────────────────────────────────────
CREATE TABLE IF NOT EXISTS audit_logs (
    id         UUID        PRIMARY KEY DEFAULT gen_random_uuid(),
    agent_id   TEXT,
    event_type TEXT        NOT NULL,
    details    JSONB       NOT NULL DEFAULT '{}',
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);
CREATE INDEX IF NOT EXISTS idx_audit_agent   ON audit_logs(agent_id);
CREATE INDEX IF NOT EXISTS idx_audit_created ON audit_logs(created_at DESC);
