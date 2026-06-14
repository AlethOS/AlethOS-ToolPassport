-- Stage 6: Persist immutable frozen Evidence Boards and canonical Manifests.

CREATE TABLE evidence_boards (
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    version INTEGER NOT NULL CHECK (version > 0),
    board_json TEXT NOT NULL CHECK (json_valid(board_json) AND json_type(board_json) = 'object'),
    frozen_at TEXT NOT NULL,
    PRIMARY KEY (run_id, version)
);

CREATE TABLE evidence_manifests (
    run_id TEXT NOT NULL,
    evidence_board_version INTEGER NOT NULL CHECK (evidence_board_version > 0),
    manifest_json TEXT NOT NULL CHECK (
        json_valid(manifest_json) AND json_type(manifest_json) = 'object'
    ),
    PRIMARY KEY (run_id, evidence_board_version),
    FOREIGN KEY (run_id, evidence_board_version)
        REFERENCES evidence_boards(run_id, version)
        ON DELETE RESTRICT
);

CREATE TRIGGER evidence_boards_prevent_update
BEFORE UPDATE ON evidence_boards
BEGIN
    SELECT RAISE(ABORT, 'evidence_boards are immutable');
END;

CREATE TRIGGER evidence_boards_prevent_delete
BEFORE DELETE ON evidence_boards
BEGIN
    SELECT RAISE(ABORT, 'evidence_boards are immutable');
END;

CREATE TRIGGER evidence_manifests_prevent_update
BEFORE UPDATE ON evidence_manifests
BEGIN
    SELECT RAISE(ABORT, 'evidence_manifests are immutable');
END;

CREATE TRIGGER evidence_manifests_prevent_delete
BEFORE DELETE ON evidence_manifests
BEGIN
    SELECT RAISE(ABORT, 'evidence_manifests are immutable');
END;
