from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_audit_catalog import validate_instance
from validate_tool_identity import (
    INTAKE_SCHEMA,
    RESOLUTION_SCHEMA,
    TOOL_SCHEMA,
    ToolIdentityValidationError,
    normalize_url,
    resolve_identity,
    validate_contract_schemas,
    validate_fixture,
    validate_resolution,
    validate_tool,
)


class ToolIdentityContractTests(unittest.TestCase):
    def setUp(self) -> None:
        self.tool = {
            "tool_schema_version": "0.1.0",
            "tool_id": "github:example/tool",
            "name": "Example Tool",
            "tool_type": "mcp_server",
            "canonical_url": "https://github.com/example/tool",
            "external_identifiers": [
                {
                    "namespace": "github",
                    "value": "example/tool",
                    "canonical_url": "https://github.com/example/tool",
                }
            ],
            "aliases": ["Earlier Name"],
        }

    def test_committed_fixtures_are_valid(self) -> None:
        paths = sorted((ROOT / "fixtures" / "tool-identity" / "0.1.0").glob("*.json"))
        self.assertGreaterEqual(len(paths), 10)
        for path in paths:
            with self.subTest(path=path.name):
                validate_fixture(path)

    def test_tool_types_match_committed_profile_catalog(self) -> None:
        self.assertEqual(validate_contract_schemas(), [])

    def test_github_variants_normalize_to_one_identifier(self) -> None:
        expected = {
            "namespace": "github",
            "value": "langchain-ai/langgraph",
            "canonical_url": "https://github.com/langchain-ai/langgraph",
        }
        for raw_url in (
            "https://GitHub.com/LangChain-AI/LangGraph",
            "https://github.com/langchain-ai/langgraph.git/",
            "https://github.com/langchain-ai/langgraph/",
        ):
            self.assertEqual(normalize_url(raw_url), expected)

    def test_general_url_preserves_path_case(self) -> None:
        self.assertEqual(
            normalize_url("https://Example.com:443/Tool/"),
            {
                "namespace": "url",
                "value": "example.com/Tool",
                "canonical_url": "https://example.com/Tool",
            },
        )
        self.assertNotEqual(
            normalize_url("https://example.com/Tool"),
            normalize_url("https://example.com/tool"),
        )

    def test_ambiguous_urls_are_not_normalized(self) -> None:
        for raw_url in (
            "http://example.com/tool",
            "https://example.com:8443/tool",
            "https://example.com./tool",
            "https://example.com\\@other.example/tool",
            "https://example.com/tool?query=1",
            "https://example.com/tool#section",
            "https://github.com/example/tool/issues/1",
        ):
            self.assertIsNone(normalize_url(raw_url))

    def test_tool_type_is_bound_to_profile_catalog(self) -> None:
        intake = {
            "intake_version": "0.1.0",
            "name": "Tool",
            "tool_type": "workflow_platform",
            "urls": [],
        }
        errors = validate_instance(intake, INTAKE_SCHEMA, "intake")
        self.assertTrue(any("not in the allowed enum" in error for error in errors))

    def test_tool_schema_is_strict(self) -> None:
        tool = copy.deepcopy(self.tool)
        tool["unexpected"] = True
        errors = validate_instance(tool, TOOL_SCHEMA, "tool")
        self.assertTrue(any("unexpected property" in error for error in errors))

    def test_external_identifier_pairs_must_be_unique(self) -> None:
        tool = copy.deepcopy(self.tool)
        duplicate_pair = copy.deepcopy(tool["external_identifiers"][0])
        duplicate_pair["canonical_url"] = "https://github.com/example/other"
        tool["external_identifiers"].append(duplicate_pair)
        errors = validate_tool(tool)
        self.assertIn(
            "tool: external identifier namespace/value pairs must be unique",
            errors,
        )

    def test_tool_id_must_be_backed_by_external_identifier(self) -> None:
        tool = copy.deepcopy(self.tool)
        tool["tool_id"] = "github:other/tool"
        errors = validate_tool(tool)
        self.assertIn("tool: tool_id must match one external identifier", errors)

    def test_external_identifier_cannot_be_owned_by_multiple_tools(self) -> None:
        migrated = copy.deepcopy(self.tool)
        migrated["external_identifiers"].append(
            {
                "namespace": "github",
                "value": "new-org/tool",
                "canonical_url": "https://github.com/new-org/tool",
            }
        )
        other = {
            "tool_schema_version": "0.1.0",
            "tool_id": "github:new-org/tool",
            "name": "Other Tool",
            "tool_type": "mcp_server",
            "canonical_url": "https://github.com/new-org/tool",
            "external_identifiers": [
                {
                    "namespace": "github",
                    "value": "new-org/tool",
                    "canonical_url": "https://github.com/new-org/tool",
                }
            ],
            "aliases": [],
        }
        fixture = {
            "fixture_version": "0.1.0",
            "scenario_id": "duplicate_owner",
            "description": "One strong identifier cannot belong to two Tools.",
            "existing_tools": [migrated, other],
            "intake": {
                "intake_version": "0.1.0",
                "name": "Unknown",
                "tool_type": "generic",
                "urls": [],
            },
            "expected_resolution": {
                "resolution_version": "0.1.0",
                "status": "needs_review",
                "normalized_identifiers": [],
                "tool_id": None,
                "candidate_tool_ids": [],
                "reason_codes": ["name_only"],
            },
        }
        with tempfile.TemporaryDirectory() as directory:
            path = Path(directory) / "fixture.json"
            path.write_text(json.dumps(fixture), encoding="utf-8")
            with self.assertRaisesRegex(
                ToolIdentityValidationError,
                "owned by multiple Tools",
            ):
                validate_fixture(path)

    def test_alias_only_match_never_resolves_automatically(self) -> None:
        intake = {
            "intake_version": "0.1.0",
            "name": "Earlier Name",
            "tool_type": "mcp_server",
            "urls": [],
        }
        resolution = resolve_identity(intake, [self.tool])
        self.assertEqual(resolution["status"], "needs_review")
        self.assertIsNone(resolution["tool_id"])
        self.assertEqual(resolution["reason_codes"], ["name_match_only"])

    def test_resolution_state_invariants_are_enforced(self) -> None:
        invalid = {
            "resolution_version": "0.1.0",
            "status": "resolved",
            "normalized_identifiers": [],
            "tool_id": "github:missing/tool",
            "candidate_tool_ids": [],
            "reason_codes": ["existing_identifier_match"],
        }
        errors = validate_resolution(invalid, [self.tool])
        self.assertTrue(any("must reference an existing Tool" in error for error in errors))
        self.assertTrue(any("must be included in candidate_tool_ids" in error for error in errors))

        invalid["status"] = "needs_review"
        errors = validate_resolution(invalid, [self.tool])
        self.assertTrue(any("must not select or propose" in error for error in errors))

        invalid = {
            "resolution_version": "0.1.0",
            "status": "create_candidate",
            "normalized_identifiers": [],
            "tool_id": "github:new/tool",
            "candidate_tool_ids": [],
            "reason_codes": ["new_strong_identifier"],
        }
        errors = validate_resolution(invalid, [self.tool])
        self.assertTrue(any("exactly one strong identifier" in error for error in errors))

    def test_approved_migration_keeps_original_tool_id(self) -> None:
        migrated = copy.deepcopy(self.tool)
        migrated["canonical_url"] = "https://github.com/new-org/tool"
        migrated["external_identifiers"].append(
            {
                "namespace": "github",
                "value": "new-org/tool",
                "canonical_url": "https://github.com/new-org/tool",
            }
        )
        intake = {
            "intake_version": "0.1.0",
            "name": "Example Tool",
            "tool_type": "mcp_server",
            "urls": ["https://github.com/new-org/tool"],
        }
        resolution = resolve_identity(intake, [migrated])
        self.assertEqual(resolution["status"], "resolved")
        self.assertEqual(resolution["tool_id"], "github:example/tool")

    def test_resolution_schema_rejects_unqualified_tool_id(self) -> None:
        resolution = {
            "resolution_version": "0.1.0",
            "status": "needs_review",
            "normalized_identifiers": [],
            "tool_id": "missing-namespace",
            "candidate_tool_ids": [],
            "reason_codes": ["name_only"],
        }
        errors = validate_instance(resolution, RESOLUTION_SCHEMA, "resolution")
        self.assertTrue(any("does not match pattern" in error for error in errors))


if __name__ == "__main__":
    unittest.main()
