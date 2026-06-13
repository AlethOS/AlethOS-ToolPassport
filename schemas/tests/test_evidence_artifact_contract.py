from __future__ import annotations

import copy
import sys
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_audit_catalog import load_json, validate_instance


class EvidenceArtifactContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.artifact_schema = load_json(ROOT / "schemas" / "artifact.schema.json")
        self.evidence_create_schema = load_json(ROOT / "schemas" / "evidence-create.schema.json")
        self.evidence_schema = load_json(ROOT / "schemas" / "evidence.schema.json")
        self.evidence_create = {
            "evidence_schema_version": "0.2.0",
            "source_type": "official_docs",
            "source_url": "https://example.com/docs",
            "source_revision": "v1.0.0",
            "title": "Interface documentation",
            "excerpt": "Structured JSON is supported.",
            "retrieved_at": "2026-06-12T20:00:00Z",
            "snapshot_artifact_id": None,
            "supports": ["check.structured_io"],
            "contradicts": [],
            "metadata": {"section": "API"},
        }

    def test_artifact_contract_accepts_rust_owned_metadata(self) -> None:
        artifact = {
            "artifact_schema_version": "0.1.0",
            "artifact_id": "11111111-1111-4111-8111-111111111111",
            "run_id": "22222222-2222-4222-8222-222222222222",
            "filename": "snapshot.html",
            "content_type": "text/html",
            "size_bytes": 42,
            "sha256_hash": f"0x{'a' * 64}",
            "created_at": "2026-06-12T20:00:01Z",
        }
        self.assertEqual(validate_instance(artifact, self.artifact_schema, "artifact"), [])

        artifact["storage_key"] = "private/path"
        self.assertIn("unexpected property 'storage_key'", "\n".join(
            validate_instance(artifact, self.artifact_schema, "artifact")
        ))

    def test_evidence_create_contract_rejects_trust_core_fields(self) -> None:
        self.assertEqual(
            validate_instance(self.evidence_create, self.evidence_create_schema, "evidence"),
            [],
        )

        for field in ("evidence_id", "run_id", "content_hash", "created_at"):
            payload = copy.deepcopy(self.evidence_create)
            payload[field] = "caller-controlled"
            with self.subTest(field=field):
                self.assertIn(
                    f"unexpected property '{field}'",
                    "\n".join(
                        validate_instance(payload, self.evidence_create_schema, "evidence")
                    ),
                )

    def test_stored_evidence_contract_requires_hash_and_run_binding(self) -> None:
        stored = {
            **self.evidence_create,
            "evidence_id": "11111111-1111-4111-8111-111111111111",
            "run_id": "22222222-2222-4222-8222-222222222222",
            "size_bytes": 128,
            "content_hash": f"0x{'b' * 64}",
            "created_at": "2026-06-12T20:00:01Z",
        }
        self.assertEqual(validate_instance(stored, self.evidence_schema, "evidence"), [])

        invalid_hash = copy.deepcopy(stored)
        invalid_hash["content_hash"] = "not-a-hash"
        self.assertIn(
            "does not match pattern",
            "\n".join(validate_instance(invalid_hash, self.evidence_schema, "evidence")),
        )

    def test_evidence_references_are_unique(self) -> None:
        payload = copy.deepcopy(self.evidence_create)
        payload["supports"] = ["check.a", "check.a"]
        self.assertIn(
            "items must be unique",
            "\n".join(validate_instance(payload, self.evidence_create_schema, "evidence")),
        )


if __name__ == "__main__":
    unittest.main()
