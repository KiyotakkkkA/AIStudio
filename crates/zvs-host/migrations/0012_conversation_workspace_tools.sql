UPDATE `conversation`
SET `enabled_tools` = (
  SELECT json_group_array(`key`) FROM (
    SELECT `value` AS `key` FROM json_each(`conversation`.`enabled_tools`)
    UNION ALL
    SELECT `added`.`value` FROM json_each('["fs.list","fs.read","fs.search","fs.apply","code.run","shell.run"]') AS `added`
    WHERE `added`.`value` NOT IN (SELECT `value` FROM json_each(`conversation`.`enabled_tools`))
  )
);
