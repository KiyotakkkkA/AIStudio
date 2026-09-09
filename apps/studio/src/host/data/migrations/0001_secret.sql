CREATE TABLE `secret` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`name` text NOT NULL,
	`scope` text NOT NULL,
	`cipher` blob,
	`cipher_version` integer DEFAULT 1 NOT NULL,
	`hint` text,
	`fields` text DEFAULT '{}' NOT NULL,
	`tags` text DEFAULT '[]' NOT NULL,
	`note` text,
	`rotation_days` integer,
	`rotates_at` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);
--> statement-breakpoint
CREATE INDEX `secret_type_idx` ON `secret` (`type`);--> statement-breakpoint
CREATE INDEX `secret_scope_idx` ON `secret` (`scope`);--> statement-breakpoint
CREATE UNIQUE INDEX `secret_name_type_unq` ON `secret` (`name`,`type`);--> statement-breakpoint
CREATE TABLE `secret_usage` (
	`secret_id` text NOT NULL,
	`consumer_kind` text NOT NULL,
	`consumer_id` text NOT NULL,
	`created_at` integer NOT NULL,
	PRIMARY KEY(`secret_id`, `consumer_kind`, `consumer_id`),
	FOREIGN KEY (`secret_id`) REFERENCES `secret`(`id`) ON UPDATE cascade ON DELETE restrict
);
