-- Stage 8: Persist immutable receipts independently from frozen Passports.

CREATE TABLE attestation_attempts (
    run_id TEXT PRIMARY KEY NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    approval_id TEXT NOT NULL UNIQUE REFERENCES approvals(approval_id) ON DELETE RESTRICT,
    claimed_at TEXT NOT NULL
);

CREATE TRIGGER attestation_attempts_prevent_update
BEFORE UPDATE ON attestation_attempts
BEGIN
    SELECT RAISE(ABORT, 'attestation attempts are immutable');
END;

CREATE TRIGGER attestation_attempts_prevent_delete
BEFORE DELETE ON attestation_attempts
BEGIN
    SELECT RAISE(ABORT, 'attestation attempts are immutable');
END;

CREATE TABLE attestation_receipts (
    attestation_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL UNIQUE REFERENCES runs(run_id) ON DELETE RESTRICT,
    receipt_json TEXT NOT NULL CHECK (
        json_valid(receipt_json) AND json_type(receipt_json) = 'object'
    ),
    submitted_at TEXT NOT NULL,
    confirmed_at TEXT
);

CREATE TRIGGER attestation_receipts_prevent_update
BEFORE UPDATE ON attestation_receipts
BEGIN
    SELECT RAISE(ABORT, 'attestation receipts are immutable');
END;

CREATE TRIGGER attestation_receipts_prevent_delete
BEFORE DELETE ON attestation_receipts
BEGIN
    SELECT RAISE(ABORT, 'attestation receipts are immutable');
END;
