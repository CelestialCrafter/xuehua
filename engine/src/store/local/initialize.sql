BEGIN;
CREATE TABLE IF NOT EXISTS artifacts(
    hash BLOB PRIMARY KEY NOT NULL,
    created_at TEXT
);
CREATE TABLE IF NOT EXISTS packages(
    hash BLOB PRIMARY KEY NOT NULL,
    artifact BLOB NOT NULL,
    created_at TEXT,
    FOREIGN KEY(artifact) REFERENCES artifacts(hash)
);
COMMIT;
