UPDATE `secret`
SET `note` = json_extract(`fields`, '$.description')
WHERE `type` = 'custom'
	AND (`note` IS NULL OR trim(`note`) = '')
	AND trim(coalesce(json_extract(`fields`, '$.description'), '')) <> '';
UPDATE `secret`
SET `name` = `name` || ' · ' || `type`
WHERE `type` IN ('ollama', 'openrouter', 'mistral', 'custom')
	AND EXISTS (
		SELECT 1 FROM `secret` AS `other`
		WHERE `other`.`name` = `secret`.`name`
			AND `other`.`type` IN ('ollama', 'openrouter', 'mistral', 'custom')
			AND `other`.`type` < `secret`.`type`
	);
UPDATE `secret` SET `type` = 'user' WHERE `type` IN ('ollama', 'openrouter', 'mistral', 'custom');
ALTER TABLE `secret` DROP COLUMN `fields`;
