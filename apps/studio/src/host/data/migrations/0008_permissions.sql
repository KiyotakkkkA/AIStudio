CREATE TABLE `pending_approval` (
	`id` text PRIMARY KEY NOT NULL,
	`run_id` text NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`expires_at` integer NOT NULL,
	`decision` text
);
--> statement-breakpoint
CREATE TABLE `permission_use` (
	`id` text PRIMARY KEY NOT NULL,
	`grant_id` text NOT NULL,
	`run_id` text NOT NULL,
	`used_at` integer NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`tier` text NOT NULL,
	`granted_by` text NOT NULL,
	`granted_at` integer NOT NULL
);
--> statement-breakpoint
CREATE TABLE `tool_permission` (
	`id` text PRIMARY KEY NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`tier` text NOT NULL,
	`granted_at` integer NOT NULL,
	`granted_by` text NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `tool_permission_subject_scope` ON `tool_permission` (`subject`,`scope`);