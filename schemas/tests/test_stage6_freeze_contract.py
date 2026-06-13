from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_audit_catalog import load_json, validate_instance

RUN_ID = "11111111-1111-4111-8111-111111111111"
CHECK_RESULTS_ID = "22222222-2222-4222-8222-222222222222"
EVIDENCE_ID = "33333333-3333-4333-8333-333333333333"
ATTESTATION_ID = "44444444-4444-4444-8444-444444444444"
HASH_A = f"0x{'a' * 64}"
HASH_B = f"0x{'b' * 64}"
HASH_C = f"0x{'c' * 64}"
HASH_D = f"0x{'d' * 64}"


def dimension_scores() -> dict[str, dict[str, int | float]]:
    return {
        dimension: {
            "score": 3,
            "earned_weighted_points": 0.75,
            "applicable_weight": 1.25,
        }
        for dimension in (
            "capability_clarity",
            "interface_openness",
            "automation_readiness",
            "data_portability",
            "permission_risk",
            "evidence_quality",
            "ecosystem_fit",
        )
    }


class Stage6FreezeContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.submission_schema = load_json(
            ROOT / "schemas" / "check-results-submission.schema.json"
        )
        self.results_schema = load_json(ROOT / "schemas" / "check-results.schema.json")
        self.board_schema = load_json(ROOT / "schemas" / "evidence-board.schema.json")
        self.manifest_schema = load_json(ROOT / "schemas" / "evidence-manifest.schema.json")
        self.passport_v01_schema = load_json(ROOT / "schemas" / "passport.schema.json")
        self.passport_schema = load_json(ROOT / "schemas" / "passport-v0.2.schema.json")
        self.provenance_schema = load_json(ROOT / "schemas" / "provenance.schema.json")
        self.receipt_schema = load_json(
            ROOT / "schemas" / "attestation-receipt.schema.json"
        )

        self.submission = {
            "check_results_schema_version": "0.1.0",
            "evidence_board_version": 1,
            "findings": [
                {
                    "check_id": "agent_framework.structured_io",
                    "finding": "partial",
                    "rationale": "One official source supports structured output.",
                    "evidence_ids": [EVIDENCE_ID],
                    "not_applicable_reason": None,
                }
            ],
        }
        self.results = {
            "check_results_schema_version": "0.1.0",
            "check_results_id": CHECK_RESULTS_ID,
            "run_id": RUN_ID,
            "evidence_board_version": 1,
            "standard_id": "alethos-toolpassport",
            "standard_version": "0.2.0",
            "profile_id": "agent_framework",
            "profile_version": "0.2.0",
            "results": [
                {
                    **self.submission["findings"][0],
                    "dimension": "automation_readiness",
                    "weight": 1.25,
                    "high_risk": False,
                    "scoring_rule_id": "positive_capability_v1",
                    "rule_points": 0.5,
                    "weighted_points": 0.625,
                    "applicable": True,
                }
            ],
            "dimension_scores": dimension_scores(),
            "total_score": 60,
            "rating": "trial",
            "computed_at": "2026-06-13T00:00:00Z",
        }
        self.board = {
            "evidence_board_schema_version": "0.1.0",
            "run_id": RUN_ID,
            "version": 1,
            "standard_id": "alethos-toolpassport",
            "standard_version": "0.2.0",
            "profile_id": "agent_framework",
            "profile_version": "0.2.0",
            "evidence_ids": [EVIDENCE_ID],
            "claims": [
                {
                    "claim_id": "claim.structured_io",
                    "check_id": "agent_framework.structured_io",
                    "statement": "The framework exposes structured output.",
                    "status": "partially_supported",
                    "confidence": 0.6,
                    "supports": [EVIDENCE_ID],
                    "contradicts": [],
                }
            ],
            "gaps": [
                {
                    "gap_id": "gap.structured_io",
                    "check_id": "agent_framework.structured_io",
                    "description": "A second independent example is missing.",
                    "priority": "medium",
                    "status": "accepted",
                    "resolution": "Frozen with insufficient evidence.",
                }
            ],
            "freeze_reason": "Research budget reached with reviewable gaps.",
            "frozen_at": "2026-06-13T00:00:01Z",
        }
        self.manifest = {
            "evidence_manifest_schema_version": "0.1.0",
            "run_id": RUN_ID,
            "evidence_board_version": 1,
            "entries": [
                {
                    "evidence_id": EVIDENCE_ID,
                    "content_hash": HASH_A,
                    "snapshot_artifact_id": None,
                    "snapshot_hash": None,
                }
            ],
        }
        self.passport = {
            "passport_version": "0.2.0",
            "passport_sequence": 1,
            "tool_id": "github:langchain-ai/langgraph",
            "run_id": RUN_ID,
            "tool_type": "agent_framework",
            "target_revision": "commit:abc123",
            "audit_scope": "Mock audit of documented automation behavior.",
            "standard_id": "alethos-toolpassport",
            "standard_version": "0.2.0",
            "profile_id": "agent_framework",
            "profile_version": "0.2.0",
            "evidence_board_version": 1,
            "check_results_id": CHECK_RESULTS_ID,
            "capability_claims": [
                {
                    "statement_id": "claim.structured_io",
                    "statement": "The framework exposes structured output.",
                    "status": "partially_supported",
                    "evidence_ids": [EVIDENCE_ID],
                }
            ],
            "interfaces": [],
            "risks": [],
            "known_gaps": ["A second independent example is missing."],
            "scores": {
                "dimensions": {
                    dimension: score["score"]
                    for dimension, score in dimension_scores().items()
                },
                "total_score": 60,
                "rating": "trial",
            },
            "recommendation": {
                "summary": "Use only after validating recovery behavior.",
                "conditions": ["Validate recovery behavior."],
            },
        }

    def test_finding_submission_excludes_rust_owned_scores_and_hashes(self) -> None:
        self.assertEqual(
            validate_instance(self.submission, self.submission_schema, "submission"),
            [],
        )
        for field in (
            "run_id",
            "dimension_scores",
            "total_score",
            "rating",
            "passport_hash",
        ):
            payload = copy.deepcopy(self.submission)
            payload[field] = "caller-controlled"
            with self.subTest(field=field):
                self.assertIn(
                    f"unexpected property '{field}'",
                    "\n".join(
                        validate_instance(payload, self.submission_schema, "submission")
                    ),
                )

    def test_stored_results_require_rust_owned_aggregates(self) -> None:
        self.assertEqual(validate_instance(self.results, self.results_schema, "results"), [])
        payload = copy.deepcopy(self.results)
        del payload["total_score"]
        self.assertIn(
            "missing required property 'total_score'",
            "\n".join(validate_instance(payload, self.results_schema, "results")),
        )

    def test_frozen_board_and_manifest_are_strict_and_run_bound(self) -> None:
        self.assertEqual(validate_instance(self.board, self.board_schema, "board"), [])
        self.assertEqual(
            validate_instance(self.manifest, self.manifest_schema, "manifest"),
            [],
        )
        payload = copy.deepcopy(self.manifest)
        payload["entries"][0]["source_excerpt"] = "not part of the commitment"
        self.assertIn(
            "unexpected property 'source_excerpt'",
            "\n".join(validate_instance(payload, self.manifest_schema, "manifest")),
        )

    def test_passport_v02_has_no_attestation_or_commitment_fields(self) -> None:
        self.assertEqual(
            validate_instance(self.passport, self.passport_schema, "passport"),
            [],
        )
        for field in (
            "web3_attestation",
            "passport_hash",
            "audit_log_hash",
            "evidence_manifest_hash",
        ):
            payload = copy.deepcopy(self.passport)
            payload[field] = {}
            with self.subTest(field=field):
                self.assertIn(
                    f"unexpected property '{field}'",
                    "\n".join(validate_instance(payload, self.passport_schema, "passport")),
                )

    def test_passport_v01_remains_available_for_historical_reads(self) -> None:
        self.assertEqual(self.passport_v01_schema["properties"]["passport_version"]["const"], "0.1")
        self.assertIn("web3_attestation", self.passport_v01_schema["required"])

    def test_provenance_binds_all_freeze_commitments(self) -> None:
        provenance = {
            "provenance_schema_version": "0.1.0",
            "run_id": RUN_ID,
            "freeze_version": 1,
            "evidence_board_version": 1,
            "passport_sequence": 1,
            "passport_hash": HASH_A,
            "audit_log_hash": HASH_B,
            "evidence_manifest_hash": HASH_C,
            "onchain_run_id": HASH_D,
            "frozen_at": "2026-06-13T00:00:02Z",
        }
        self.assertEqual(
            validate_instance(provenance, self.provenance_schema, "provenance"),
            [],
        )

    def test_attestation_receipt_is_independent_and_commitment_bound(self) -> None:
        receipt = {
            "attestation_receipt_schema_version": "0.1.0",
            "attestation_id": ATTESTATION_ID,
            "run_id": RUN_ID,
            "tool_id": "github:langchain-ai/langgraph",
            "passport_hash": HASH_A,
            "audit_log_hash": HASH_B,
            "evidence_manifest_hash": HASH_C,
            "onchain_run_id": HASH_D,
            "chain_id": 11155111,
            "registry_contract": f"0x{'1' * 40}",
            "status": "submitted",
            "transaction_hash": f"0x{'2' * 64}",
            "submitted_at": "2026-06-13T00:00:03Z",
            "confirmed_at": None,
        }
        self.assertEqual(validate_instance(receipt, self.receipt_schema, "receipt"), [])
        receipt["passport"] = copy.deepcopy(self.passport)
        self.assertIn(
            "unexpected property 'passport'",
            "\n".join(validate_instance(receipt, self.receipt_schema, "receipt")),
        )


if __name__ == "__main__":
    unittest.main()
