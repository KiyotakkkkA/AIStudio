CREATE TABLE `integration_account_next` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`name` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`config` text DEFAULT '{}' NOT NULL,
	`last_error` text,
	`last_activity_at` integer,
	`event_count` integer DEFAULT 0 NOT NULL,
	`duplicate_count` integer DEFAULT 0 NOT NULL,
	`run_count` integer DEFAULT 0 NOT NULL,
	`tool_call_count` integer DEFAULT 0 NOT NULL,
	`error_count` integer DEFAULT 0 NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	CHECK (`kind` IN ('mail', 'telegram', 'github', 'gitlab'))
);
INSERT INTO `integration_account_next` SELECT * FROM `integration_account` WHERE `kind` <> 'git';
CREATE TEMP TABLE `kept_integration_event` AS SELECT * FROM `integration_event`;
CREATE TEMP TABLE `kept_integration_activity` AS SELECT * FROM `integration_activity`;
CREATE TEMP TABLE `kept_integration_cursor` AS SELECT * FROM `integration_cursor`;
DROP TABLE `integration_account`;
ALTER TABLE `integration_account_next` RENAME TO `integration_account`;
INSERT INTO `integration_event`
	SELECT * FROM `kept_integration_event`
	WHERE `account_id` IN (SELECT `id` FROM `integration_account`);
INSERT INTO `integration_activity`
	SELECT * FROM `kept_integration_activity`
	WHERE `account_id` IN (SELECT `id` FROM `integration_account`);
INSERT INTO `integration_cursor`
	SELECT * FROM `kept_integration_cursor`
	WHERE `account_id` IN (SELECT `id` FROM `integration_account`);
DROP TABLE `kept_integration_event`;
DROP TABLE `kept_integration_activity`;
DROP TABLE `kept_integration_cursor`;
