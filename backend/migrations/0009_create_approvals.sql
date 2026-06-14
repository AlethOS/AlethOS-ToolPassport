-- Stage 8: Persist immutable human decisions bound to frozen provenance.

CREATE TABLE approvals (
    approval_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL UNIQUE REFERENCES runs(run_id) ON DELETE RESTRICT,
    approval_json TEXT NOT NULL CHECK (
        json_valid(approval_json) AND json_type(approval_json) = 'object'
    ),
    decided_at TEXT NOT NULL
);

CREATE TRIGGER approvals_prevent_update
BEFORE UPDATE ON approvals
BEGIN
    SELECT RAISE(ABORT, 'approvals are immutable');
END;

CREATE TRIGGER approvals_prevent_delete
BEFORE DELETE ON approvals
BEGIN
    SELECT RAISE(ABORT, 'approvals are immutable');
END;
