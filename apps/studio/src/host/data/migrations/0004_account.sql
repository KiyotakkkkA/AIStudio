CREATE TABLE `account` (
	`id` text PRIMARY KEY NOT NULL,
	`adapter` text NOT NULL,
	`partition` text NOT NULL,
	`external_id` text NOT NULL,
	`email_masked` text,
	`display_name` text,
	`avatar_url` text,
	`token_secret_id` text,
	`token_expires_at` integer,
	`status` text DEFAULT 'linked' NOT NULL,
	`status_detail` text,
	`last_checked_at` integer,
	`linked_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`token_secret_id`) REFERENCES `secret`(`id`) ON UPDATE no action ON DELETE set null
);
--> statement-breakpoint
CREATE UNIQUE INDEX `account_adapter_external_unq` ON `account` (`adapter`,`external_id`);--> statement-breakpoint
CREATE INDEX `account_adapter_idx` ON `account` (`adapter`);--> statement-breakpoint
ALTER TABLE `provider` ADD `account_id` text REFERENCES account(id);