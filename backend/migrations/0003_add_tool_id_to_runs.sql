-- Stage 2.3: Bind Run to canonical Tool identity.
-- tool_id references the authoritative Tool in the registry.
-- canonical_url is the frozen snapshot URL from the Tool at creation time.
-- Both are NOT NULL for new rows; existing rows (if any) get empty defaults
-- and must be migrated explicitly by the application layer.
ALTER TABLE runs ADD COLUMN tool_id TEXT NOT NULL DEFAULT '';
ALTER TABLE runs ADD COLUMN canonical_url TEXT NOT NULL DEFAULT '';
