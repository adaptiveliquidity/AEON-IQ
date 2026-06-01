-- Migration 0013: change FLOAT (float8) columns to REAL (float4) to match f32 Rust types
-- Required for sqlx 0.9 strict type checking.

ALTER TABLE memories
    ALTER COLUMN confidence TYPE REAL USING confidence::REAL;

ALTER TABLE entities
    ALTER COLUMN confidence TYPE REAL USING confidence::REAL;

ALTER TABLE memory_graph
    ALTER COLUMN confidence TYPE REAL USING confidence::REAL;
