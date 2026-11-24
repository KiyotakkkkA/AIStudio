CREATE TEMP TABLE dialog_changes (owner TEXT NOT NULL, kind TEXT NOT NULL, id TEXT NOT NULL, position INTEGER);
CREATE TEMP TABLE dialog_stream (owner TEXT NOT NULL, id TEXT NOT NULL, content TEXT NOT NULL, reasoning TEXT NOT NULL, tokens_out INTEGER NOT NULL, duration_ms INTEGER NOT NULL);

CREATE TEMP TRIGGER dialog_created AFTER INSERT ON main.conversation BEGIN
    INSERT INTO dialog_changes VALUES (new.id, 'metadata', new.id, NULL);
END;
CREATE TEMP TRIGGER dialog_changed AFTER UPDATE ON main.conversation BEGIN
    INSERT INTO dialog_changes VALUES (new.id, 'metadata', new.id, NULL);
END;
CREATE TEMP TRIGGER dialog_removed AFTER DELETE ON main.conversation BEGIN
    INSERT INTO dialog_changes VALUES (old.id, 'delete_dialog', old.id, NULL);
END;
CREATE TEMP TRIGGER dialog_message_created AFTER INSERT ON main.message BEGIN
    INSERT INTO dialog_changes VALUES (new.conversation_id, 'message', new.id, NULL);
END;
CREATE TEMP TRIGGER dialog_message_changed AFTER UPDATE OF id, conversation_id, role, sources,
    images, selections, commands, plugins, offered_tools, rounds, tokens_in, tokens_cached,
    tokens_reasoning, usage_estimated, run_id, partial, pinned, event, created_at, recap ON main.message BEGIN
    INSERT INTO dialog_changes VALUES (new.conversation_id, 'message', new.id, NULL);
END;
CREATE TEMP TRIGGER dialog_message_content AFTER UPDATE OF content, reasoning, tokens_out, duration_ms ON main.message BEGIN
    INSERT INTO dialog_stream
    SELECT new.conversation_id, new.id,
        coalesce(CAST(substr(CAST(new.content AS BLOB), length(CAST(old.content AS BLOB)) + 1) AS TEXT), ''),
        coalesce(CAST(substr(CAST(new.reasoning AS BLOB), length(CAST(old.reasoning AS BLOB)) + 1) AS TEXT), ''),
        new.tokens_out, new.duration_ms
    WHERE instr(CAST(new.content AS BLOB), CAST(old.content AS BLOB)) = 1
      AND instr(CAST(new.reasoning AS BLOB), CAST(old.reasoning AS BLOB)) = 1;
    INSERT INTO dialog_changes
    SELECT new.conversation_id, 'message', new.id, NULL
    WHERE instr(CAST(new.content AS BLOB), CAST(old.content AS BLOB)) != 1
       OR instr(CAST(new.reasoning AS BLOB), CAST(old.reasoning AS BLOB)) != 1;
END;
CREATE TEMP TRIGGER dialog_message_removed AFTER DELETE ON main.message BEGIN
    INSERT INTO dialog_changes VALUES (old.conversation_id, 'remove_message', old.id, NULL);
END;
CREATE TEMP TRIGGER dialog_call_created AFTER INSERT ON main.message_tool_call BEGIN
    INSERT INTO dialog_changes SELECT conversation_id, 'tool_call', new.message_id, new.idx FROM message WHERE id = new.message_id;
END;
CREATE TEMP TRIGGER dialog_call_changed AFTER UPDATE ON main.message_tool_call BEGIN
    INSERT INTO dialog_changes SELECT conversation_id, 'tool_call', new.message_id, new.idx FROM message WHERE id = new.message_id;
END;
CREATE TEMP TRIGGER dialog_compaction_created AFTER INSERT ON main.chat_compaction BEGIN
    INSERT INTO dialog_changes VALUES (new.conversation_id, 'compaction', new.id, NULL);
END;
CREATE TEMP TRIGGER dialog_compaction_removed AFTER DELETE ON main.chat_compaction BEGIN
    INSERT INTO dialog_changes VALUES (old.conversation_id, 'remove_compaction', old.id, NULL);
END;
