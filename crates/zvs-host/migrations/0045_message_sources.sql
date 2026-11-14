ALTER TABLE `message` ADD `sources` text DEFAULT '[]' NOT NULL;
UPDATE `message` SET `sources` = (
	SELECT json_group_array(json(`s`.`value`) ORDER BY `s`.`rank`, `s`.`idx`)
	FROM (
		SELECT json_set(`c`.`value`, '$.kind', 'vector') AS `value`, 0 AS `rank`, CAST(`c`.`key` AS integer) AS `idx`
		FROM json_each(`message`.`citations`) AS `c`
		UNION ALL
		SELECT json_set(`w`.`value`, '$.kind', 'web'), 1, CAST(`w`.`key` AS integer)
		FROM json_each(`message`.`web_sources`) AS `w`
	) AS `s`
)
WHERE json_array_length(`citations`) > 0 OR json_array_length(`web_sources`) > 0;
ALTER TABLE `message` DROP COLUMN `citations`;
ALTER TABLE `message` DROP COLUMN `web_sources`;
