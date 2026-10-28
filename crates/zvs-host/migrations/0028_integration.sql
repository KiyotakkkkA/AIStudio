CREATE TABLE `integration_account` (
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
	CHECK (`kind` IN ('mail', 'telegram', 'git'))
);

CREATE TABLE `integration_event` (
	`account_id` text NOT NULL REFERENCES `integration_account` (`id`) ON DELETE CASCADE,
	`trigger` text NOT NULL,
	`event_id` text NOT NULL,
	`received_at` integer NOT NULL,
	PRIMARY KEY (`account_id`, `trigger`, `event_id`)
);

CREATE INDEX `integration_event_received_idx` ON `integration_event` (`received_at`);

CREATE TABLE `integration_activity` (
	`id` integer PRIMARY KEY AUTOINCREMENT NOT NULL,
	`account_id` text NOT NULL REFERENCES `integration_account` (`id`) ON DELETE CASCADE,
	`at` integer NOT NULL,
	`level` text NOT NULL,
	`message` text NOT NULL,
	CHECK (`level` IN ('info', 'warn', 'error'))
);

CREATE INDEX `integration_activity_account_idx` ON `integration_activity` (`account_id`, `id`);
