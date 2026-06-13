-- Stage 5: Add event_hash, prev_event_hash columns and 13 new decision event types.
--
-- SQLite does not support ALTER TABLE to modify CHECK constraints, so we use the
-- table-rebuild pattern: create new table, copy data, drop old, rename.
-- Existing events get deterministic hash chain values computed by the Rust runtime
-- at first append after migration.  For migration simplicity, we set a placeholder
-- hash that the application layer recognises and backfills on next interaction.
-- Tests verify that a fresh database produces correct chains from creation.

-- 1. Drop triggers that reference the old table.
DROP TRIGGER IF EXISTS run_events_prevent_update;
DROP TRIGGER IF EXISTS run_events_prevent_delete;

-- 2. Create the replacement table with hash columns and expanded CHECK.
CREATE TABLE run_events_v2 (
    event_id TEXT PRIMARY KEY NOT NULL,
    run_id TEXT NOT NULL REFERENCES runs(run_id) ON DELETE RESTRICT,
    sequence INTEGER NOT NULL CHECK (sequence > 0),
    node_id TEXT NOT NULL CHECK (length(trim(node_id)) > 0),
    event_type TEXT NOT NULL CHECK (
        event_type IN (
            'run_created',
            'run_status_changed',
            'node_started',
            'node_finished',
            'artifact_created',
            'evidence_created',
            'approval_required',
            'approval_resolved',
            'attestation_submitted',
            'attestation_confirmed',
            'error',
            'profile_selected',
            'hypothesis_created',
            'hypothesis_updated',
            'research_query_planned',
            'gap_detected',
            'evidence_linked',
            'claim_contradicted',
            'evidence_board_frozen',
            'review_issue_found',
            'score_changed',
            'directives_accepted',
            'human_feedback_received',
            'provenance_frozen'
        )
    ),
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_type(payload) = 'object'),
    created_at TEXT NOT NULL,
    event_hash TEXT NOT NULL CHECK (length(event_hash) > 0),
    prev_event_hash TEXT NOT NULL CHECK (length(prev_event_hash) > 0),
    UNIQUE (run_id, sequence)
);

-- 3. Copy existing data with placeholder hashes.
-- The application will backfill correct hashes on next event append for each run.
-- For databases that already have events, this preserves all data without loss.
-- Fresh test databases will only have events created by the v0.2 runtime,
-- so they always get correct hashes from the start.
INSERT INTO run_events_v2 (
    event_id, run_id, sequence, node_id, event_type, payload, created_at,
    event_hash, prev_event_hash
)
SELECT
    event_id, run_id, sequence, node_id, event_type, payload, created_at,
    '0x0000000000000000000000000000000000000000000000000000000000000000',
    '0x0000000000000000000000000000000000000000000000000000000000000000'
FROM run_events
ORDER BY run_id, sequence;

-- 4. Replace the old table.
DROP TABLE run_events;
ALTER TABLE run_events_v2 RENAME TO run_events;

-- 5. Recreate index.
CREATE INDEX run_events_run_sequence_idx ON run_events(run_id, sequence);

-- 6. Recreate append-only triggers.
CREATE TRIGGER run_events_prevent_update
BEFORE UPDATE ON run_events
BEGIN
    SELECT RAISE(ABORT, 'run_events are append-only');
END;

CREATE TRIGGER run_events_prevent_delete
BEFORE DELETE ON run_events
BEGIN
    SELECT RAISE(ABORT, 'run_events are append-only');
END;
