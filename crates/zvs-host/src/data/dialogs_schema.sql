CREATE TABLE IF NOT EXISTS conversation (
    id TEXT PRIMARY KEY NOT NULL,
    title TEXT NOT NULL,
    provider_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    settings TEXT NOT NULL,
    system_prompt TEXT NOT NULL,
    attached_store_ids TEXT NOT NULL,
    use_relations INTEGER NOT NULL DEFAULT 0,
    retrieval_mode TEXT NOT NULL DEFAULT 'hybrid',
    working_folder TEXT,
    turn_folder TEXT,
    profile TEXT NOT NULL DEFAULT '',
    pinned INTEGER NOT NULL DEFAULT 0,
    goal TEXT,
    created_at INTEGER NOT NULL,
    updated_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS dialog_order_idx ON conversation (pinned DESC, updated_at DESC, id DESC);
CREATE INDEX IF NOT EXISTS dialog_workspace_order_idx ON conversation (working_folder, pinned DESC, updated_at DESC, id DESC);

CREATE TABLE IF NOT EXISTS message (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    role TEXT NOT NULL,
    content TEXT NOT NULL,
    reasoning TEXT NOT NULL DEFAULT '',
    sources TEXT NOT NULL DEFAULT '[]',
    images TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(images)),
    selections TEXT NOT NULL DEFAULT '[]',
    commands TEXT NOT NULL DEFAULT '[]',
    plugins TEXT NOT NULL DEFAULT '[]',
    offered_tools TEXT NOT NULL DEFAULT '[]',
    rounds TEXT NOT NULL DEFAULT '[]',
    tokens_in INTEGER NOT NULL,
    tokens_out INTEGER NOT NULL,
    tokens_cached INTEGER NOT NULL DEFAULT 0,
    tokens_reasoning INTEGER NOT NULL DEFAULT 0,
    usage_estimated INTEGER NOT NULL DEFAULT 1,
    duration_ms INTEGER NOT NULL,
    run_id TEXT,
    partial INTEGER NOT NULL DEFAULT 0,
    pinned INTEGER NOT NULL DEFAULT 0,
    event TEXT,
    created_at INTEGER NOT NULL,
    recap TEXT
) STRICT;
CREATE INDEX IF NOT EXISTS message_conversation_created_idx ON message (conversation_id, created_at, id);
CREATE INDEX IF NOT EXISTS message_pinned_idx ON message (conversation_id, created_at, id) WHERE pinned = 1;
CREATE INDEX IF NOT EXISTS message_activity_idx ON message (created_at, run_id)
    WHERE role = 'assistant' AND run_id IS NOT NULL AND partial = 0;

CREATE TABLE IF NOT EXISTS message_tool_call (
    message_id TEXT NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    idx INTEGER NOT NULL,
    call TEXT NOT NULL,
    PRIMARY KEY (message_id, idx)
) STRICT;

CREATE TABLE IF NOT EXISTS chat_compaction (
    id TEXT PRIMARY KEY NOT NULL,
    conversation_id TEXT NOT NULL REFERENCES conversation(id) ON DELETE CASCADE,
    boundary_message_id TEXT NOT NULL REFERENCES message(id) ON DELETE CASCADE,
    summary TEXT NOT NULL,
    messages INTEGER NOT NULL CHECK(messages > 0),
    tokens_before INTEGER NOT NULL,
    tokens_after INTEGER NOT NULL,
    provider_id TEXT NOT NULL,
    model_id TEXT NOT NULL,
    trigger TEXT NOT NULL CHECK(trigger IN ('manual', 'auto')),
    created_at INTEGER NOT NULL
) STRICT;
CREATE INDEX IF NOT EXISTS chat_compaction_conversation_idx ON chat_compaction (conversation_id, created_at, id);
CREATE INDEX IF NOT EXISTS chat_compaction_boundary_idx ON chat_compaction (boundary_message_id);

CREATE TABLE IF NOT EXISTS dialog_checkpoint (id TEXT PRIMARY KEY NOT NULL, offset INTEGER NOT NULL) STRICT;
