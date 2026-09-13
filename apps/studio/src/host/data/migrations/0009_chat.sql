CREATE TABLE `conversation` (
	`id` text PRIMARY KEY NOT NULL,
	`title` text NOT NULL,
	`provider_id` text NOT NULL,
	`model_id` text NOT NULL,
	`settings` text NOT NULL,
	`system_prompt` text NOT NULL,
	`attached_store_ids` text NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);
--> statement-breakpoint
CREATE TABLE `message` (
	`id` text PRIMARY KEY NOT NULL,
	`conversation_id` text NOT NULL,
	`role` text NOT NULL,
	`content` text NOT NULL,
	`citations` text NOT NULL,
	`tokens_in` integer NOT NULL,
	`tokens_out` integer NOT NULL,
	`usage_estimated` integer DEFAULT true NOT NULL,
	`duration_ms` integer NOT NULL,
	`run_id` text,
	`partial` integer DEFAULT false NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE set null
);
--> statement-breakpoint
CREATE INDEX `message_conversation_created_idx` ON `message` (`conversation_id`,`created_at`,`id`);