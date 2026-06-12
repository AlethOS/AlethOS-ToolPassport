CREATE TABLE runs (
    run_id TEXT PRIMARY KEY NOT NULL,
    goal TEXT NOT NULL CHECK (length(trim(goal)) > 0),
    tool_name TEXT NOT NULL CHECK (length(trim(tool_name)) > 0),
    tool_type TEXT NOT NULL CHECK (length(trim(tool_type)) > 0),
    tool_urls TEXT NOT NULL CHECK (json_valid(tool_urls) AND json_type(tool_urls) = 'array'),
    status TEXT NOT NULL CHECK (
        status IN (
            'pending',
            'running',
            'success',
            'failed',
            'waiting_approval',
            'cancelled'
        )
    ),
    current_node TEXT,
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE run_events (
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
            'error'
        )
    ),
    payload TEXT NOT NULL CHECK (json_valid(payload) AND json_type(payload) = 'object'),
    created_at TEXT NOT NULL,
    UNIQUE (run_id, sequence)
);

CREATE INDEX run_events_run_sequence_idx ON run_events(run_id, sequence);

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
