CREATE TABLE IF NOT EXISTS channel_cache (
    -- channel_id is the hash of the channel's first event
    creation_event_hash TEXT PRIMARY KEY,
    creator_pubkey BLOB NOT NULL,
    -- UNIX milliseconds
    created_at INTEGER NOT NULL,
    updated_event_hash BLOB,
    -- UNIX milliseconds
    updated_at INTEGER,
    -- METADATA JSON CONTENT (name, about, picture)
    metadata TEXT NOT NULL
);

-- Channel Cache Indexes
CREATE INDEX IF NOT EXISTS updated_event_hash_index ON channel_cache(updated_event_hash);