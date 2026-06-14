-- Stage 6: Persist immutable Passport v0.2 documents and frozen audit Provenance.

-- The Passport document carries immutable audit content and Rust-owned scores,
-- but no commitment hashes. The Provenance record holds the four Rust-owned
-- commitment hashes (passport_hash, audit_log_hash, evidence_manifest_hash,
-- onchain_run_id). Both are append-only: a re-freeze after more research writes
-- a new (run_id, sequence) / (run_id, freeze_version) row instead of mutating.

CREATE TABLE passports (
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    passport_json TEXT NOT NULL CHECK (
        json_valid(passport_json) AND json_type(passport_json) = 'object'
    ),
    frozen_at TEXT NOT NULL,
    PRIMARY KEY (run_id, sequence)
);

CREATE TABLE provenances (
    run_id TEXT NOT NULL,
    freeze_version INTEGER NOT NULL CHECK (freeze_version > 0),
    provenance_json TEXT NOT NULL CHECK (
        json_valid(provenance_json) AND json_type(provenance_json) = 'object'
    ),
    PRIMARY KEY (run_id, freeze_version),
    FOREIGN KEY (run_id, freeze_version)
        REFERENCES passports(run_id, sequence)
        ON DELETE RESTRICT
);

CREATE TRIGGER passports_prevent_update
BEFORE UPDATE ON passports
BEGIN
    SELECT RAISE(ABORT, 'passports are immutable');
END;

CREATE TRIGGER passports_prevent_delete
BEFORE DELETE ON passports
BEGIN
    SELECT RAISE(ABORT, 'passports are immutable');
END;

CREATE TRIGGER provenances_prevent_update
BEFORE UPDATE ON provenances
BEGIN
    SELECT RAISE(ABORT, 'provenances are immutable');
END;

CREATE TRIGGER provenances_prevent_delete
BEFORE DELETE ON provenances
BEGIN
    SELECT RAISE(ABORT, 'provenances are immutable');
END;
