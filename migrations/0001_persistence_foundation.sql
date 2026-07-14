PRAGMA foreign_keys = ON;

CREATE TABLE shelves (
    id TEXT PRIMARY KEY NOT NULL,
    token_hash BLOB NOT NULL,
    state TEXT NOT NULL CHECK (state IN ('active', 'expiring')),
    revision INTEGER NOT NULL DEFAULT 0 CHECK (revision >= 0),
    created_at TEXT NOT NULL,
    last_seen_at TEXT NOT NULL,
    last_activity_at TEXT NOT NULL,
    expires_at TEXT NOT NULL,
    hard_expires_at TEXT
);

CREATE TABLE books (
    id TEXT PRIMARY KEY NOT NULL,
    shelf_id TEXT NOT NULL REFERENCES shelves(id) ON DELETE CASCADE,
    status TEXT NOT NULL CHECK (status IN ('pending', 'ready', 'deleting')),
    title TEXT NOT NULL,
    author TEXT,
    filename TEXT NOT NULL,
    original_name TEXT NOT NULL,
    stored_filename TEXT NOT NULL,
    size INTEGER NOT NULL CHECK (size >= 0),
    uploaded_at TEXT NOT NULL,
    UNIQUE (shelf_id, stored_filename)
);

CREATE INDEX books_shelf_status_uploaded
    ON books (shelf_id, status, uploaded_at DESC);
