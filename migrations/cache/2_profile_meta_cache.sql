CREATE TABLE IF NOT EXISTS profile_meta_cache (
    public_key BLOB PRIMARY KEY,
    -- UNIX milliseconds
    updated_at INTEGER NOT NULL,
    event_hash TEXT NOT NULL,
    from_relay TEXT NOT NULL,
    -- METADATA JSON CONTENT
    metadata TEXT NOT NULL
);