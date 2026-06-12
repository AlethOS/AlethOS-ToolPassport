# Audit Standards

This directory contains versioned, read-only Audit Standard fixtures. Each
fixture must validate against `schemas/audit-standard.schema.json`.

The Standard defines stable dimensions, allowed evidence types, and scoring
rule identifiers. Profiles may reference these identifiers but may not create
new dimensions or scoring rules.
