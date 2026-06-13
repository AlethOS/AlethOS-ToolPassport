"""Run Event v0.2 schema contract tests."""
from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_audit_catalog import load_json, validate_instance

ZERO_HASH = "0x" + "0" * 64
VALID_SHA256 = "0x" + "a" * 64

V01_EVENT_TYPES = [
    "run_created",
    "run_status_changed",
    "node_started",
    "node_finished",
    "artifact_created",
    "evidence_created",
    "approval_required",
    "approval_resolved",
    "attestation_submitted",
    "attestation_confirmed",
    "error",
]

V02_DECISION_EVENT_TYPES = [
    "profile_selected",
    "hypothesis_created",
    "hypothesis_updated",
    "research_query_planned",
    "gap_detected",
    "evidence_linked",
    "claim_contradicted",
    "evidence_board_frozen",
    "review_issue_found",
    "score_changed",
    "directives_accepted",
    "human_feedback_received",
    "provenance_frozen",
]

ALL_V02_EVENT_TYPES = V01_EVENT_TYPES + V02_DECISION_EVENT_TYPES

VALID_RUN_EVENT = {
    "event_id": "11111111-1111-4111-8111-111111111111",
    "run_id": "22222222-2222-4222-8222-222222222222",
    "sequence": 1,
    "node_id": "trust_core",
    "event_type": "run_created",
    "payload": {},
    "created_at": "2026-06-12T20:00:00Z",
    "event_hash": VALID_SHA256,
    "prev_event_hash": ZERO_HASH,
}


class RunEventV02ContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.schema = load_json(ROOT / "schemas" / "run-event.schema.json")

    def test_valid_v02_run_created_passes(self) -> None:
        errors = validate_instance(VALID_RUN_EVENT, self.schema, "run-event")
        self.assertEqual(errors, [])

    def test_valid_v02_with_nonempty_payload(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["payload"] = {"status": "success"}
        self.assertEqual(validate_instance(event, self.schema, "run-event"), [])

    def test_missing_sequence_is_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        del event["sequence"]
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "missing sequence must be rejected")

    def test_missing_event_hash_is_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        del event["event_hash"]
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "missing event_hash must be rejected")

    def test_missing_prev_event_hash_is_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        del event["prev_event_hash"]
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "missing prev_event_hash must be rejected")

    def test_invalid_event_hash_format_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["event_hash"] = "not-a-hash"
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "invalid event_hash format must be rejected")

    def test_invalid_prev_event_hash_format_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["prev_event_hash"] = "not-a-hash"
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "invalid prev_event_hash format must be rejected")

    def test_zero_prev_event_hash_is_valid(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["prev_event_hash"] = ZERO_HASH
        self.assertEqual(validate_instance(event, self.schema, "run-event"), [])

    def test_unknown_event_type_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["event_type"] = "unknown_event"
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "unknown event_type must be rejected")

    def test_v01_event_without_hash_fields_rejected(self) -> None:
        """v0.1 events missing sequence, event_hash, prev_event_hash must fail v0.2."""
        v01_event = {
            "event_id": "11111111-1111-4111-8111-111111111111",
            "run_id": "22222222-2222-4222-8222-222222222222",
            "node_id": "trust_core",
            "event_type": "run_created",
            "payload": {},
            "created_at": "2026-06-12T20:00:00Z",
        }
        errors = validate_instance(v01_event, self.schema, "run-event")
        self.assertTrue(errors, "v0.1 event must be rejected under v0.2 schema")

    def test_sequence_zero_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["sequence"] = 0
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "sequence 0 must be rejected")

    def test_sequence_negative_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["sequence"] = -1
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "negative sequence must be rejected")

    def test_additional_properties_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["extra_field"] = "not allowed"
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "additional properties must be rejected")

    def test_empty_node_id_rejected(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["node_id"] = ""
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "empty node_id must be rejected")

    def test_payload_must_be_object(self) -> None:
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["payload"] = "not an object"
        errors = validate_instance(event, self.schema, "run-event")
        self.assertTrue(errors, "non-object payload must be rejected")


class RunEventV02DecisionEventTypesTest(unittest.TestCase):
    """Each v0.2 decision event type must be accepted by the schema."""
    def setUp(self) -> None:
        self.schema = load_json(ROOT / "schemas" / "run-event.schema.json")

    def test_all_v01_types_accepted(self) -> None:
        for event_type in V01_EVENT_TYPES:
            event = copy.deepcopy(VALID_RUN_EVENT)
            event["event_type"] = event_type
            event["sequence"] = 2
            errors = validate_instance(event, self.schema, "run-event")
            self.assertEqual(errors, [], f"{event_type} should be accepted")

    def test_all_v02_decision_types_accepted(self) -> None:
        for event_type in V02_DECISION_EVENT_TYPES:
            event = copy.deepcopy(VALID_RUN_EVENT)
            event["event_type"] = event_type
            event["sequence"] = 3
            errors = validate_instance(event, self.schema, "run-event")
            self.assertEqual(errors, [], f"{event_type} should be accepted")

    def test_decision_event_with_structured_payload(self) -> None:
        """Decision events should carry structured decision summaries."""
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["event_type"] = "profile_selected"
        event["sequence"] = 2
        event["payload"] = {
            "profile_id": "mcp_server@0.2.0",
            "reason": "Tool exposes MCP protocol endpoints",
        }
        self.assertEqual(validate_instance(event, self.schema, "run-event"), [])

    def test_provenance_frozen_with_freeze_payload(self) -> None:
        """provenance_frozen should accept passport_hash in payload."""
        event = copy.deepcopy(VALID_RUN_EVENT)
        event["event_type"] = "provenance_frozen"
        event["sequence"] = 10
        event["payload"] = {
            "passport_hash": "0x" + "b" * 64,
            "freeze_version": 1,
        }
        self.assertEqual(validate_instance(event, self.schema, "run-event"), [])


if __name__ == "__main__":
    unittest.main()
