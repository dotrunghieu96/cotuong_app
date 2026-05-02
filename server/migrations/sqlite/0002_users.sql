CREATE TABLE users (
    id              TEXT PRIMARY KEY NOT NULL,
    username        TEXT NOT NULL UNIQUE COLLATE NOCASE,
    email           TEXT NOT NULL UNIQUE COLLATE NOCASE,
    email_verified  INTEGER NOT NULL DEFAULT 0,
    password_hash   TEXT,
    oauth_provider  TEXT,
    oauth_subject   TEXT,
    created_at      TEXT NOT NULL,
    last_seen_at    TEXT,
    UNIQUE (oauth_provider, oauth_subject)
);

CREATE TABLE sessions (
    token_hash   TEXT PRIMARY KEY NOT NULL,
    user_id      TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    created_at   TEXT NOT NULL,
    expires_at   TEXT NOT NULL,
    user_agent   TEXT
);

CREATE INDEX sessions_user_id_idx ON sessions (user_id);
CREATE INDEX sessions_expires_at_idx ON sessions (expires_at);

CREATE TABLE email_tokens (
    token_hash  TEXT PRIMARY KEY NOT NULL,
    user_id     TEXT NOT NULL REFERENCES users(id) ON DELETE CASCADE,
    purpose     TEXT NOT NULL CHECK (purpose IN ('verify_email', 'password_reset')),
    created_at  TEXT NOT NULL,
    expires_at  TEXT NOT NULL,
    used_at     TEXT
);

CREATE INDEX email_tokens_user_id_idx ON email_tokens (user_id);

ALTER TABLE games ADD COLUMN red_user_id   TEXT REFERENCES users(id);
ALTER TABLE games ADD COLUMN black_user_id TEXT REFERENCES users(id);
