from __future__ import annotations

import copy
import json
import sys
import tempfile
import unittest
from pathlib import Path

ROOT = Path(__file__).resolve().parents[2]
sys.path.insert(0, str(ROOT / "scripts"))

from validate_audit_catalog import CatalogValidationError, load_json, validate_catalog


class AuditCatalogValidationTests(unittest.TestCase):
    def setUp(self) -> None:
        self.standard = load_json(ROOT / "standards" / "alethos-toolpassport" / "0.2.0.json")
        self.generic = load_json(ROOT / "profiles" / "generic" / "0.2.0.json")
        self.agent_framework = load_json(ROOT / "profiles" / "agent_framework" / "0.2.0.json")
        self.mcp_server = load_json(ROOT / "profiles" / "mcp_server" / "0.2.0.json")
        self.cli_api_tool = load_json(ROOT / "profiles" / "cli_api_tool" / "0.2.0.json")

    def validate(
        self,
        *,
        standard: dict | None = None,
        profiles: list[dict] | None = None,
    ) -> None:
        standard = standard or copy.deepcopy(self.standard)
        profiles = profiles or [
            copy.deepcopy(self.generic),
            copy.deepcopy(self.agent_framework),
            copy.deepcopy(self.mcp_server),
            copy.deepcopy(self.cli_api_tool),
        ]

        with tempfile.TemporaryDirectory() as directory:
            root = Path(directory)
            standard_path = root / "standard.json"
            standard_path.write_text(json.dumps(standard), encoding="utf-8")
            profile_paths: list[Path] = []
            for index, profile in enumerate(profiles):
                profile_path = root / f"profile-{index}.json"
                profile_path.write_text(json.dumps(profile), encoding="utf-8")
                profile_paths.append(profile_path)

            validate_catalog([standard_path], profile_paths)

    def assert_invalid(
        self,
        expected: str,
        *,
        standard: dict | None = None,
        profiles: list[dict] | None = None,
    ) -> None:
        with self.assertRaisesRegex(CatalogValidationError, expected):
            self.validate(standard=standard, profiles=profiles)

    def test_committed_catalog_is_valid(self) -> None:
        validate_catalog(
            [ROOT / "standards" / "alethos-toolpassport" / "0.2.0.json"],
            [
                ROOT / "profiles" / "generic" / "0.2.0.json",
                ROOT / "profiles" / "agent_framework" / "0.2.0.json",
                ROOT / "profiles" / "mcp_server" / "0.2.0.json",
                ROOT / "profiles" / "cli_api_tool" / "0.2.0.json",
            ],
        )

    def test_committed_catalog_has_unique_candidates_and_one_fallback(self) -> None:
        profiles = [self.generic, self.agent_framework, self.mcp_server, self.cli_api_tool]
        candidates = [
            candidate
            for profile in profiles
            for candidate in profile["selection"]["tool_type_candidates"]
        ]
        fallbacks = [profile["profile_id"] for profile in profiles if profile["selection"]["fallback"]]

        self.assertEqual(len(candidates), len(set(candidates)))
        self.assertEqual(fallbacks, ["generic"])

    def test_mcp_server_declares_key_permission_checks_as_high_risk(self) -> None:
        checks = {check["check_id"]: check for check in self.mcp_server["checks"]}

        for check_id in (
            "mcp_server.filesystem_shell_boundary",
            "mcp_server.network_credential_scope",
        ):
            self.assertTrue(checks[check_id]["high_risk"])
            self.assertEqual(checks[check_id]["scoring_rule_id"], "bounded_risk_v1")

    def test_cli_api_tool_declares_side_effect_checks_as_high_risk(self) -> None:
        checks = {check["check_id"]: check for check in self.cli_api_tool["checks"]}

        for check_id in (
            "cli_api_tool.credential_scope",
            "cli_api_tool.destructive_action_control",
            "cli_api_tool.paid_side_effect_control",
        ):
            self.assertTrue(checks[check_id]["high_risk"])
            self.assertEqual(checks[check_id]["scoring_rule_id"], "bounded_risk_v1")

    def test_invalid_version_is_rejected(self) -> None:
        standard = copy.deepcopy(self.standard)
        standard["standard_version"] = "v0.2"
        self.assert_invalid("does not match pattern", standard=standard)

    def test_duplicate_check_id_is_rejected(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["checks"].append(copy.deepcopy(generic["checks"][0]))
        self.assert_invalid(
            "duplicate check_id",
            profiles=[generic, copy.deepcopy(self.agent_framework)],
        )

    def test_unknown_dimension_is_rejected(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["checks"][0]["dimension"] = "unknown_dimension"
        self.assert_invalid(
            "unknown dimension",
            profiles=[generic, copy.deepcopy(self.agent_framework)],
        )

    def test_unknown_scoring_rule_is_rejected(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["checks"][0]["scoring_rule_id"] = "unknown_rule"
        self.assert_invalid(
            "unknown scoring_rule_id", profiles=[generic, copy.deepcopy(self.agent_framework)]
        )

    def test_unknown_evidence_type_is_rejected(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["checks"][0]["required_evidence_types"] = ["unknown_evidence"]
        self.assert_invalid(
            "unknown evidence_type_id", profiles=[generic, copy.deepcopy(self.agent_framework)]
        )

    def test_non_positive_weight_is_rejected(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["checks"][0]["weight"] = 0
        self.assert_invalid(
            "value must be greater than 0",
            profiles=[generic, copy.deepcopy(self.agent_framework)],
        )

    def test_only_generic_can_be_fallback(self) -> None:
        agent_framework = copy.deepcopy(self.agent_framework)
        agent_framework["selection"]["fallback"] = True
        self.assert_invalid(
            "only the generic profile may be the fallback",
            profiles=[copy.deepcopy(self.generic), agent_framework],
        )

    def test_fallback_must_declare_scope_limitations(self) -> None:
        generic = copy.deepcopy(self.generic)
        generic["scope_limitations"] = []
        self.assert_invalid(
            "fallback profile must declare scope limitations",
            profiles=[generic, copy.deepcopy(self.agent_framework)],
        )

    def test_selection_candidates_must_include_profile_id(self) -> None:
        agent_framework = copy.deepcopy(self.agent_framework)
        agent_framework["selection"]["tool_type_candidates"] = ["agent_runtime"]
        self.assert_invalid(
            "selection candidates must include the profile_id",
            profiles=[copy.deepcopy(self.generic), agent_framework],
        )


if __name__ == "__main__":
    unittest.main()
