#!/usr/bin/env python3
"""Offline validator for versioned Audit Standard and Profile fixtures."""

from __future__ import annotations

import argparse
import json
import re
import sys
from pathlib import Path
from typing import Any

ROOT = Path(__file__).resolve().parents[1]
STANDARD_SCHEMA_PATH = ROOT / "schemas" / "audit-standard.schema.json"
PROFILE_SCHEMA_PATH = ROOT / "schemas" / "audit-profile.schema.json"


class CatalogValidationError(ValueError):
    """Raised when catalog fixtures fail structural or semantic validation."""

    def __init__(self, errors: list[str]) -> None:
        self.errors = tuple(errors)
        super().__init__("\n".join(errors))


def load_json(path: Path) -> Any:
    with path.open(encoding="utf-8") as handle:
        return json.load(handle)


def _resolve_local_ref(root_schema: dict[str, Any], reference: str) -> dict[str, Any]:
    if not reference.startswith("#/"):
        raise ValueError(f"unsupported non-local schema reference: {reference}")

    current: Any = root_schema
    for segment in reference[2:].split("/"):
        segment = segment.replace("~1", "/").replace("~0", "~")
        current = current[segment]
    if not isinstance(current, dict):
        raise ValueError(f"schema reference does not resolve to an object: {reference}")
    return current


def _matches_type(instance: Any, expected: str) -> bool:
    if expected == "object":
        return isinstance(instance, dict)
    if expected == "array":
        return isinstance(instance, list)
    if expected == "string":
        return isinstance(instance, str)
    if expected == "boolean":
        return isinstance(instance, bool)
    if expected == "number":
        return isinstance(instance, (int, float)) and not isinstance(instance, bool)
    if expected == "integer":
        return isinstance(instance, int) and not isinstance(instance, bool)
    if expected == "null":
        return instance is None
    raise ValueError(f"unsupported schema type: {expected}")


def _validate_instance(
    instance: Any,
    schema: dict[str, Any],
    root_schema: dict[str, Any],
    path: str,
    errors: list[str],
) -> None:
    if "$ref" in schema:
        _validate_instance(
            instance,
            _resolve_local_ref(root_schema, schema["$ref"]),
            root_schema,
            path,
            errors,
        )
        return

    expected_type = schema.get("type")
    if expected_type is not None:
        expected_types = [expected_type] if isinstance(expected_type, str) else expected_type
        if not any(_matches_type(instance, item) for item in expected_types):
            errors.append(f"{path}: expected type {expected_type!r}")
            return

    if "const" in schema and instance != schema["const"]:
        errors.append(f"{path}: expected constant {schema['const']!r}")
    if "enum" in schema and instance not in schema["enum"]:
        errors.append(f"{path}: value is not in the allowed enum")

    if isinstance(instance, dict):
        required = schema.get("required", [])
        for name in required:
            if name not in instance:
                errors.append(f"{path}: missing required property {name!r}")

        properties = schema.get("properties", {})
        for name, value in instance.items():
            if name in properties:
                _validate_instance(value, properties[name], root_schema, f"{path}.{name}", errors)
            elif schema.get("additionalProperties") is False:
                errors.append(f"{path}: unexpected property {name!r}")

    if isinstance(instance, list):
        if len(instance) < schema.get("minItems", 0):
            errors.append(f"{path}: expected at least {schema['minItems']} item(s)")
        if schema.get("uniqueItems"):
            serialized = [json.dumps(item, sort_keys=True, separators=(",", ":")) for item in instance]
            if len(set(serialized)) != len(serialized):
                errors.append(f"{path}: items must be unique")
        item_schema = schema.get("items")
        if item_schema is not None:
            for index, value in enumerate(instance):
                _validate_instance(value, item_schema, root_schema, f"{path}[{index}]", errors)

    if isinstance(instance, str):
        if len(instance) < schema.get("minLength", 0):
            errors.append(f"{path}: string is shorter than {schema['minLength']}")
        pattern = schema.get("pattern")
        if pattern is not None and re.fullmatch(pattern, instance) is None:
            errors.append(f"{path}: value does not match pattern {pattern!r}")

    if isinstance(instance, (int, float)) and not isinstance(instance, bool):
        if "minimum" in schema and instance < schema["minimum"]:
            errors.append(f"{path}: value must be at least {schema['minimum']}")
        if "maximum" in schema and instance > schema["maximum"]:
            errors.append(f"{path}: value must be at most {schema['maximum']}")
        if "exclusiveMinimum" in schema and instance <= schema["exclusiveMinimum"]:
            errors.append(f"{path}: value must be greater than {schema['exclusiveMinimum']}")


def validate_instance(instance: Any, schema: dict[str, Any], label: str) -> list[str]:
    errors: list[str] = []
    _validate_instance(instance, schema, schema, label, errors)
    return errors


def _duplicate_ids(items: list[dict[str, Any]], id_field: str) -> set[str]:
    seen: set[str] = set()
    duplicates: set[str] = set()
    for item in items:
        item_id = item[id_field]
        if item_id in seen:
            duplicates.add(item_id)
        seen.add(item_id)
    return duplicates


def validate_catalog(standard_paths: list[Path], profile_paths: list[Path]) -> None:
    standard_schema = load_json(STANDARD_SCHEMA_PATH)
    profile_schema = load_json(PROFILE_SCHEMA_PATH)
    standards: dict[tuple[str, str], tuple[Path, dict[str, Any]]] = {}
    profiles: list[tuple[Path, dict[str, Any]]] = []
    errors: list[str] = []

    for path in standard_paths:
        standard = load_json(path)
        structural_errors = validate_instance(standard, standard_schema, str(path))
        errors.extend(structural_errors)
        if structural_errors:
            continue

        key = (standard["standard_id"], standard["standard_version"])
        if key in standards:
            errors.append(f"{path}: duplicate standard id/version {key!r}")
        else:
            standards[key] = (path, standard)

        for collection, id_field in (
            ("dimensions", "dimension_id"),
            ("evidence_types", "evidence_type_id"),
            ("scoring_rules", "scoring_rule_id"),
        ):
            for duplicate in sorted(_duplicate_ids(standard[collection], id_field)):
                errors.append(f"{path}: duplicate {id_field} {duplicate!r}")

        for rule in standard["scoring_rules"]:
            if rule["finding_points"]["unknown"] != 0:
                errors.append(
                    f"{path}: scoring rule {rule['scoring_rule_id']!r} must assign zero points to unknown"
                )

    for path in profile_paths:
        profile = load_json(path)
        structural_errors = validate_instance(profile, profile_schema, str(path))
        errors.extend(structural_errors)
        if not structural_errors:
            profiles.append((path, profile))

    profile_keys: set[tuple[str, str]] = set()
    claimed_candidates: dict[str, str] = {}
    claimed_check_ids: dict[str, str] = {}
    fallback_profiles: list[str] = []

    for path, profile in profiles:
        profile_key = (profile["profile_id"], profile["profile_version"])
        if profile_key in profile_keys:
            errors.append(f"{path}: duplicate profile id/version {profile_key!r}")
        profile_keys.add(profile_key)

        standard_key = (profile["standard_id"], profile["standard_version"])
        standard_entry = standards.get(standard_key)
        if standard_entry is None:
            errors.append(f"{path}: references unknown standard id/version {standard_key!r}")
            continue

        standard = standard_entry[1]
        dimensions = {item["dimension_id"] for item in standard["dimensions"]}
        evidence_types = {item["evidence_type_id"] for item in standard["evidence_types"]}
        scoring_rules = {item["scoring_rule_id"] for item in standard["scoring_rules"]}

        for duplicate in sorted(_duplicate_ids(profile["checks"], "check_id")):
            errors.append(f"{path}: duplicate check_id {duplicate!r}")

        for check in profile["checks"]:
            check_label = f"{path}: check {check['check_id']!r}"
            check_owner = claimed_check_ids.get(check["check_id"])
            if check_owner is not None:
                errors.append(
                    f"{check_label} is already declared by profile {check_owner!r}"
                )
            else:
                claimed_check_ids[check["check_id"]] = profile["profile_id"]
            if check["dimension"] not in dimensions:
                errors.append(f"{check_label} references unknown dimension {check['dimension']!r}")
            if check["scoring_rule_id"] not in scoring_rules:
                errors.append(
                    f"{check_label} references unknown scoring_rule_id {check['scoring_rule_id']!r}"
                )
            for evidence_type in check["required_evidence_types"]:
                if evidence_type not in evidence_types:
                    errors.append(
                        f"{check_label} references unknown evidence_type_id {evidence_type!r}"
                    )

        selection = profile["selection"]
        if profile["profile_id"] not in selection["tool_type_candidates"]:
            errors.append(f"{path}: selection candidates must include the profile_id")
        if selection["fallback"]:
            fallback_profiles.append(profile["profile_id"])
            if profile["profile_id"] != "generic":
                errors.append(f"{path}: only the generic profile may be the fallback")
            if selection["minimum_confidence"] != 0:
                errors.append(f"{path}: fallback profile minimum_confidence must be zero")
            if not profile["scope_limitations"]:
                errors.append(f"{path}: fallback profile must declare scope limitations")

        for candidate in selection["tool_type_candidates"]:
            owner = claimed_candidates.get(candidate)
            if owner is not None:
                errors.append(
                    f"{path}: tool type candidate {candidate!r} is already claimed by profile {owner!r}"
                )
            else:
                claimed_candidates[candidate] = profile["profile_id"]

    if fallback_profiles != ["generic"]:
        errors.append("catalog must contain exactly one generic fallback profile")

    if errors:
        raise CatalogValidationError(errors)


def _default_standard_paths() -> list[Path]:
    return sorted((ROOT / "standards").glob("*/*.json"))


def _default_profile_paths() -> list[Path]:
    return sorted((ROOT / "profiles").glob("*/*.json"))


def main() -> int:
    parser = argparse.ArgumentParser(description=__doc__)
    parser.add_argument(
        "--standard",
        action="append",
        type=Path,
        help="Audit Standard fixture path",
    )
    parser.add_argument("--profile", action="append", type=Path, help="Audit Profile fixture path")
    args = parser.parse_args()

    standard_paths = args.standard or _default_standard_paths()
    profile_paths = args.profile or _default_profile_paths()

    try:
        validate_catalog(standard_paths, profile_paths)
    except (CatalogValidationError, OSError, json.JSONDecodeError, ValueError) as error:
        print(f"FAIL audit catalog\n{error}", file=sys.stderr)
        return 1

    print(
        f"PASS audit catalog: {len(standard_paths)} standard(s), {len(profile_paths)} profile(s)"
    )
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
