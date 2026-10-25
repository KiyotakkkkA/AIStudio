DELETE FROM `tool_reference` WHERE `kind` = 'skill';

UPDATE `skill`
SET `issues` = (
    SELECT json_group_array(json(`e`.`value`)) FROM json_each(`skill`.`issues`) AS `e`
    WHERE json_extract(`e`.`value`, '$.rule')
      NOT IN ('unknownTool', 'toolOff', 'toolDisabled', 'toolUnavailable')
  ),
  `status` = CASE WHEN EXISTS (
    SELECT 1 FROM json_each(`skill`.`issues`) AS `e`
    WHERE json_extract(`e`.`value`, '$.rule')
      NOT IN ('unknownTool', 'toolOff', 'toolDisabled', 'toolUnavailable')
  ) THEN 'invalid' ELSE 'ready' END
WHERE EXISTS (
  SELECT 1 FROM json_each(`skill`.`issues`) AS `e`
  WHERE json_extract(`e`.`value`, '$.rule')
    IN ('unknownTool', 'toolOff', 'toolDisabled', 'toolUnavailable')
);

ALTER TABLE `skill` DROP COLUMN `allowed_tools`;
ALTER TABLE `skill` DROP COLUMN `model_override`;
