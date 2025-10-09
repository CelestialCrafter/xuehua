BEGIN;
CREATE TABLE IF NOT EXISTS artifacts(
    artifact BLOB PRIMARY KEY NOT NULL,
    created_at TEXT
);
CREATE TABLE IF NOT EXISTS packages(
    package TEXT PRIMARY KEY NOT NULL,
    artifact BLOB NOT NULL,
    created_at TEXT,
    FOREIGN KEY(artifact) REFERENCES artifacts(hash)
);
COMMIT;
