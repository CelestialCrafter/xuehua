BEGIN;
CREATE TABLE IF NOT EXISTS artifacts(
    artifact BLOB PRIMARY KEY NOT NULL,
    created_at TEXT
) WITHOUT ROWID;
CREATE TABLE IF NOT EXISTS packages(
    package TEXT,
    artifact BLOB NOT NULL,
    created_at TEXT,
    FOREIGN KEY(artifact) REFERENCES artifacts(hash)
    UNIQUE(package, artifact)
);
COMMIT;
