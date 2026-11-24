CREATE VIRTUAL TABLE IF NOT EXISTS message_search USING fts5(content, content = 'message', prefix = '2 3');
CREATE TRIGGER IF NOT EXISTS message_search_insert AFTER INSERT ON message WHEN new.partial = 0 BEGIN
    INSERT INTO message_search (rowid, content) VALUES (new.rowid, new.content);
END;
CREATE TRIGGER IF NOT EXISTS message_search_delete AFTER DELETE ON message WHEN old.partial = 0 BEGIN
    INSERT INTO message_search (message_search, rowid, content) VALUES ('delete', old.rowid, old.content);
END;
CREATE TRIGGER IF NOT EXISTS message_search_update AFTER UPDATE OF content, partial ON message
WHEN (old.partial = 0 OR new.partial = 0)
    AND (old.partial != new.partial OR old.content != new.content) BEGIN
    INSERT INTO message_search (message_search, rowid, content)
        SELECT 'delete', old.rowid, old.content WHERE old.partial = 0;
    INSERT INTO message_search (rowid, content) SELECT new.rowid, new.content WHERE new.partial = 0;
END;
