ALTER TABLE `message` ADD `commands` text DEFAULT '[]' NOT NULL;
ALTER TABLE `message` ADD `plugins` text DEFAULT '[]' NOT NULL;
UPDATE `message` SET `commands` = json_array(`command`) WHERE `command` IS NOT NULL;
ALTER TABLE `message` DROP COLUMN `command`;
