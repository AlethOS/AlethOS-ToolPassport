CREATE TABLE tools (
    tool_id TEXT PRIMARY KEY NOT NULL
        CHECK (length(trim(tool_id)) > 0),
    name TEXT NOT NULL
        CHECK (length(trim(name)) > 0),
    tool_type TEXT NOT NULL CHECK (
        tool_type IN ('generic', 'agent_framework', 'mcp_server', 'cli_api_tool')
    ),
    canonical_url TEXT NOT NULL
        CHECK (length(trim(canonical_url)) > 0),
    external_identifiers TEXT NOT NULL
        CHECK (json_valid(external_identifiers)
               AND json_type(external_identifiers) = 'array'
               AND json_array_length(external_identifiers) >= 1),
    created_at TEXT NOT NULL,
    updated_at TEXT NOT NULL
);

CREATE TABLE tool_external_ids (
    namespace TEXT NOT NULL
        CHECK (length(trim(namespace)) > 0),
    value TEXT NOT NULL
        CHECK (length(trim(value)) > 0),
    tool_id TEXT NOT NULL REFERENCES tools(tool_id) ON DELETE CASCADE,
    canonical_url TEXT NOT NULL
        CHECK (length(trim(canonical_url)) > 0),
    PRIMARY KEY (namespace, value)
);

CREATE TABLE tool_aliases (
    tool_id TEXT NOT NULL REFERENCES tools(tool_id) ON DELETE CASCADE,
    alias TEXT NOT NULL
        CHECK (length(trim(alias)) > 0),
    PRIMARY KEY (tool_id, alias)
);

CREATE INDEX tool_aliases_alias_idx ON tool_aliases(alias);
