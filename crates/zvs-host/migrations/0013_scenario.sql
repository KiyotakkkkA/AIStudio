CREATE TABLE `scenario` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`tags` text DEFAULT '[]' NOT NULL,
	`active` integer DEFAULT false NOT NULL,
	`current_version_id` text,
	`trigger` text,
	`trigger_kind` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE INDEX `scenario_trigger_idx` ON `scenario` (`active`, `trigger_kind`);

CREATE TABLE `scenario_version` (
	`id` text PRIMARY KEY NOT NULL,
	`scenario_id` text NOT NULL,
	`version` integer NOT NULL,
	`graph` text NOT NULL,
	`note` text,
	`summary` text NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`scenario_id`) REFERENCES `scenario`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `scenario_version_unq` ON `scenario_version` (`scenario_id`, `version`);
