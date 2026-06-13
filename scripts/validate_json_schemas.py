#!/usr/bin/env python3
"""Validate committed JSON Schemas against the Draft 2020-12 meta-schema."""

from __future__ import annotations

import json
import sys
from pathlib import Path

from jsonschema import Draft202012Validator
from jsonschema.exceptions import SchemaError

ROOT = Path(__file__).resolve().parents[1]
SCHEMA_DIRECTORY = ROOT / "schemas"
DRAFT_2020_12 = "https://json-schema.org/draft/2020-12/schema"


def main() -> int:
    paths = sorted(SCHEMA_DIRECTORY.glob("*.schema.json"))
    errors: list[str] = []

    for path in paths:
        try:
            schema = json.loads(path.read_text(encoding="utf-8"))
        except (OSError, json.JSONDecodeError) as error:
            errors.append(f"{path.relative_to(ROOT)}: {error}")
            continue

        if schema.get("$schema") != DRAFT_2020_12:
            errors.append(
                f"{path.relative_to(ROOT)}: $schema must be {DRAFT_2020_12!r}"
            )
            continue

        try:
            Draft202012Validator.check_schema(schema)
        except SchemaError as error:
            errors.append(f"{path.relative_to(ROOT)}: {error.message}")

    if errors:
        print("FAIL JSON Schema meta-validation", file=sys.stderr)
        for error in errors:
            print(f"- {error}", file=sys.stderr)
        return 1

    print(f"PASS JSON Schema meta-validation: {len(paths)} Draft 2020-12 schema(s)")
    return 0


if __name__ == "__main__":
    raise SystemExit(main())
