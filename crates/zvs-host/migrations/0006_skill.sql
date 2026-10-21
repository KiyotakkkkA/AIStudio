CREATE TABLE `skill` (
	`id` text PRIMARY KEY NOT NULL,
	`key` text NOT NULL,
	`name` text NOT NULL,
	`description` text NOT NULL,
	`allowed_tools` text DEFAULT '[]' NOT NULL,
	`model_override` text,
	`enabled` integer DEFAULT true NOT NULL,
	`source` text DEFAULT 'local' NOT NULL,
	`status` text DEFAULT 'ready' NOT NULL,
	`issues` text DEFAULT '[]' NOT NULL,
	`file_count` integer DEFAULT 0 NOT NULL,
	`content_hash` text NOT NULL,
	`path` text NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE UNIQUE INDEX `skill_key_unq` ON `skill` (`key`);
