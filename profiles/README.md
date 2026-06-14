# Audit Profiles

This directory contains versioned Audit Profile fixtures. Each fixture must
validate against `schemas/audit-profile.schema.json` and reference identifiers
defined by its bound Audit Standard.

Exactly one profile per catalog version is the fallback. The fallback must be
`generic`; it must declare scope limitations and use a zero confidence
threshold. Specialized profiles declare candidate tool types and a confidence
threshold. Every profile includes its own ID as a candidate, and each
candidate tool type is claimed by only one profile in that version.
