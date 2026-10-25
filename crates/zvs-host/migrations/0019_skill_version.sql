ALTER TABLE `skill` ADD `version` text;
ALTER TABLE `skill` ADD `author` text;
UPDATE `skill` SET `content_hash` = '';
