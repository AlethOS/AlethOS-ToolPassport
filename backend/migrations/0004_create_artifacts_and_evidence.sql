-- Stage 4: Evidence and Artifact Trust Core.
-- User-controlled names are metadata only. storage_key is allocated by Rust.

CREATE TABLE artifacts (
    artifact_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    filename TEXT NOT NULL CHECK (length(trim(filename)) > 0),
    content_type TEXT NOT NULL CHECK (length(trim(content_type)) > 0),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    sha256_hash TEXT NOT NULL CHECK (
        length(sha256_hash) = 66 AND sha256_hash GLOB '0x[0-9a-f]*'
    ),
    storage_key TEXT NOT NULL UNIQUE CHECK (length(trim(storage_key)) > 0),
    created_at TEXT NOT NULL
);

CREATE INDEX artifacts_run_created_idx ON artifacts(run_id, created_at, artifact_id);

CREATE TABLE evidence (
    evidence_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    source_type TEXT NOT NULL CHECK (
        source_type IN (
            'user_material',
            'official_website',
            'official_docs',
            'github_readme',
            'github_metadata',
            'public_example'
        )
    ),
    source_url TEXT NOT NULL CHECK (length(trim(source_url)) > 0),
    source_revision TEXT,
    title TEXT NOT NULL CHECK (length(trim(title)) > 0),
    excerpt TEXT NOT NULL,
    retrieved_at TEXT NOT NULL,
    snapshot_artifact_id TEXT REFERENCES artifacts(artifact_id) ON DELETE RESTRICT,
    supports TEXT NOT NULL CHECK (json_valid(supports) AND json_type(supports) = 'array'),
    contradicts TEXT NOT NULL CHECK (json_valid(contradicts) AND json_type(contradicts) = 'array'),
    metadata TEXT NOT NULL CHECK (json_valid(metadata) AND json_type(metadata) = 'object'),
    size_bytes INTEGER NOT NULL CHECK (size_bytes >= 0),
    content_hash TEXT NOT NULL CHECK (
        length(content_hash) = 66 AND content_hash GLOB '0x[0-9a-f]*'
    ),
    storage_key TEXT NOT NULL UNIQUE CHECK (length(trim(storage_key)) > 0),
    created_at TEXT NOT NULL
);

CREATE INDEX evidence_run_created_idx ON evidence(run_id, created_at, evidence_id);
