CREATE TABLE `browser_download` (
	`id` text PRIMARY KEY NOT NULL,
	`url` text NOT NULL,
	`file_name` text NOT NULL,
	`path` text,
	`bytes` integer NOT NULL,
	`state` text NOT NULL,
	`agent` integer NOT NULL,
	`finished_at` integer NOT NULL,
	CHECK (`state` IN ('done', 'failed', 'cancelled'))
);

CREATE INDEX `browser_download_finished_idx` ON `browser_download` (`finished_at`);
