CREATE TABLE contact (
    pubkey TEXT PRIMARY KEY,
    petname TEXT,
    relay_url TEXT,
    status INTEGER NOT NULL DEFAULT 0,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL,
    unseen_messages INTEGER NOT NULL DEFAULT 0,
    last_message_content TEXT,
    last_message_date INTEGER,
    profile_meta TEXT,
    profile_meta_last_update INTEGER,
    local_profile_image TEXT,
    local_banner_image TEXT
);

CREATE INDEX contact_created_at ON contact (created_at);

CREATE INDEX contact_updated_at ON contact (updated_at);

CREATE INDEX contact_status ON contact (status);