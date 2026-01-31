BEGIN;
CREATE TABLE IF NOT EXISTS artifacts(
    id BLOB PRIMARY KEY NOT NULL,
    created_at TEXT
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS packages(
    id BLOB PRIMARY KEY NOT NULL,
    artifact BLOB NOT NULL,
    created_at TEXT,
    FOREIGN KEY(artifact) REFERENCES artifacts(id)
);
COMMIT;
