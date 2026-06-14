-- Stage 6: Freeze each Run's audit catalog binding and persist immutable
-- deterministic Check Results.

-- Existing Runs were created against the historical 0.2.0 catalog. New Runs
-- explicitly write their current binding from Rust.
ALTER TABLE runs ADD COLUMN standard_id TEXT NOT NULL DEFAULT 'alethos-toolpassport';
ALTER TABLE runs ADD COLUMN standard_version TEXT NOT NULL DEFAULT '0.2.0';
ALTER TABLE runs ADD COLUMN profile_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN profile_version TEXT NOT NULL DEFAULT '0.2.0';

UPDATE runs
SET profile_id = tool_type
WHERE profile_id = '';

CREATE TRIGGER runs_prevent_audit_binding_update
BEFORE UPDATE OF standard_id, standard_version, profile_id, profile_version ON runs
BEGIN
    SELECT RAISE(ABORT, 'run audit binding is immutable');
END;

CREATE TABLE check_results (
    check_results_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    evidence_board_version INTEGER NOT NULL CHECK (evidence_board_version > 0),
    standard_id TEXT NOT NULL CHECK (length(trim(standard_id)) > 0),
    standard_version TEXT NOT NULL CHECK (length(trim(standard_version)) > 0),
    profile_id TEXT NOT NULL CHECK (length(trim(profile_id)) > 0),
    profile_version TEXT NOT NULL CHECK (length(trim(profile_version)) > 0),
    result_json TEXT NOT NULL CHECK (json_valid(result_json) AND json_type(result_json) = 'object'),
    total_score INTEGER NOT NULL CHECK (total_score >= 0 AND total_score <= 100),
    rating TEXT NOT NULL CHECK (
        rating IN (
            'not_recommended',
            'manual_only',
            'trial',
            'low_risk',
            'core_candidate'
        )
    ),
    computed_at TEXT NOT NULL,
    UNIQUE (run_id, evidence_board_version)
);

CREATE INDEX check_results_run_computed_idx
ON check_results(run_id, computed_at, check_results_id);

CREATE TRIGGER check_results_prevent_update
BEFORE UPDATE ON check_results
BEGIN
    SELECT RAISE(ABORT, 'check_results are immutable');
END;

CREATE TRIGGER check_results_prevent_delete
BEFORE DELETE ON check_results
BEGIN
    SELECT RAISE(ABORT, 'check_results are immutable');
END;
