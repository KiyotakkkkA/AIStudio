ALTER TABLE `conversation` DROP COLUMN `enabled_tools`;
ALTER TABLE `message` ADD `command` text;
UPDATE `message` SET `offered_tools` = (
	SELECT json_group_array(json_object(
		'key', json_extract(`e`.`value`, '$.key'),
		'source', CASE
			WHEN json_extract(`e`.`value`, '$.source') = 'knowledge' THEN 'knowledge'
			WHEN json_extract(`e`.`value`, '$.key') = 'chat.memory' THEN 'memory'
			ELSE 'base'
		END
	) ORDER BY CAST(`e`.`key` AS integer))
	FROM json_each(`message`.`offered_tools`) AS `e`
)
WHERE `offered_tools` <> '[]';
