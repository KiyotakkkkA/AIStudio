CREATE TABLE `integration_cursor` (
	`account_id` text NOT NULL REFERENCES `integration_account` (`id`) ON DELETE CASCADE,
	`key` text NOT NULL,
	`value` text NOT NULL,
	`updated_at` integer NOT NULL,
	PRIMARY KEY (`account_id`, `key`)
);
