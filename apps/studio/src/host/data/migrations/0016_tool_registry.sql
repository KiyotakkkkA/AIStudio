CREATE TABLE `tool` (
	`id` text PRIMARY KEY NOT NULL,
	`key` text NOT NULL,
	`source` text NOT NULL,
	`source_ref` text,
	`display_name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`input_schema` text,
	`read_only` integer DEFAULT false NOT NULL,
	`destructive` integer DEFAULT false NOT NULL,
	`tier` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`available` integer DEFAULT true NOT NULL,
	`last_used_at` integer,
	`call_count` integer DEFAULT 0 NOT NULL,
	`registered_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `tool_key_unq` ON `tool` (`key`);--> statement-breakpoint
CREATE TABLE `tool_reference` (
	`id` text PRIMARY KEY NOT NULL,
	`tool_id` text NOT NULL,
	`kind` text NOT NULL,
	`ref_id` text NOT NULL,
	`name` text NOT NULL,
	FOREIGN KEY (`tool_id`) REFERENCES `tool`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `tool_reference_unq` ON `tool_reference` (`tool_id`,`kind`,`ref_id`);