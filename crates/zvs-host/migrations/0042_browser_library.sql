CREATE TABLE `browser_history` (
	`id` text PRIMARY KEY NOT NULL,
	`url` text NOT NULL,
	`title` text NOT NULL,
	`visited_at` integer NOT NULL,
	`imported_from` text
);

CREATE UNIQUE INDEX `browser_history_visit_idx` ON `browser_history` (`url`, `visited_at`);
CREATE INDEX `browser_history_time_idx` ON `browser_history` (`visited_at`);

CREATE TABLE `browser_bookmark` (
	`id` text PRIMARY KEY NOT NULL,
	`url` text NOT NULL,
	`title` text NOT NULL,
	`folder` text NOT NULL,
	`created_at` integer NOT NULL
);

CREATE UNIQUE INDEX `browser_bookmark_url_idx` ON `browser_bookmark` (`url`);
