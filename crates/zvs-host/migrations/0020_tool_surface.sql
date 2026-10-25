CREATE TEMP TABLE `tool_rename` (
	`from_key` text NOT NULL,
	`to_key` text,
	`slot` integer NOT NULL,
	`model_only` integer NOT NULL
);

INSERT INTO `tool_rename` VALUES
	('chat.file', 'fs.read', 0, 0),
	('chat.file', 'fs.list', 1, 0),
	('chat.file', 'fs.apply', 2, 0),
	('skill.read', 'fs.read', 0, 0),
	('skill.script', 'shell.run', 0, 0),
	('knowledge.read', 'knowledge.search', 0, 0),
	('knowledge.list', 'knowledge.search', 0, 0),
	('vector.index', NULL, 0, 0),
	('job.run', NULL, 0, 0),
	('download.fetch', NULL, 0, 0),
	('vector.search', NULL, 0, 1),
	('vector.list', NULL, 0, 1);

UPDATE `conversation`
SET `enabled_tools` = (
  SELECT json_group_array(`key`) FROM (
    SELECT `key` FROM (
      SELECT CASE WHEN `r`.`from_key` IS NULL THEN `e`.`value` ELSE `r`.`to_key` END AS `key`,
        `e`.`key` * 4 + COALESCE(`r`.`slot`, 0) AS `position`
      FROM json_each(`conversation`.`enabled_tools`) AS `e`
      LEFT JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value`
    )
    WHERE `key` IS NOT NULL
    GROUP BY `key`
    ORDER BY MIN(`position`)
  )
)
WHERE EXISTS (
  SELECT 1 FROM json_each(`conversation`.`enabled_tools`) AS `e`
  JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value`
);

UPDATE `agentic_session`
SET `tools` = (
  SELECT json_group_array(`key`) FROM (
    SELECT `key` FROM (
      SELECT CASE WHEN `r`.`from_key` IS NULL THEN `e`.`value` ELSE `r`.`to_key` END AS `key`,
        `e`.`key` * 4 + COALESCE(`r`.`slot`, 0) AS `position`
      FROM json_each(`agentic_session`.`tools`) AS `e`
      LEFT JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value`
    )
    WHERE `key` IS NOT NULL
    GROUP BY `key`
    ORDER BY MIN(`position`)
  )
)
WHERE EXISTS (
  SELECT 1 FROM json_each(`agentic_session`.`tools`) AS `e`
  JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value`
);

UPDATE `skill`
SET `allowed_tools` = (
  SELECT json_group_array(`key`) FROM (
    SELECT `key` FROM (
      SELECT CASE WHEN `r`.`from_key` IS NULL THEN `e`.`value` ELSE `r`.`to_key` END AS `key`,
        `e`.`key` * 4 + COALESCE(`r`.`slot`, 0) AS `position`
      FROM json_each(`skill`.`allowed_tools`) AS `e`
      LEFT JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value` AND `r`.`model_only` = 0
    )
    WHERE `key` IS NOT NULL
    GROUP BY `key`
    ORDER BY MIN(`position`)
  )
)
WHERE EXISTS (
  SELECT 1 FROM json_each(`skill`.`allowed_tools`) AS `e`
  JOIN `tool_rename` AS `r` ON `r`.`from_key` = `e`.`value` AND `r`.`model_only` = 0
);

DELETE FROM `tool_reference`
WHERE `tool_id` IN (
  SELECT `id` FROM `tool`
  WHERE `key` IN (SELECT `from_key` FROM `tool_rename` WHERE `model_only` = 0)
);

DELETE FROM `tool_permission`
WHERE `subject` IN (SELECT `from_key` FROM `tool_rename` WHERE `to_key` IS NOT NULL);

DELETE FROM `tool`
WHERE `key` IN (SELECT `from_key` FROM `tool_rename` WHERE `model_only` = 0);

DROP TABLE `tool_rename`;
