ALTER TABLE `message` ADD `recap` text;

CREATE TABLE `chat_compaction` (
	`id` text PRIMARY KEY NOT NULL,
	`conversation_id` text NOT NULL,
	`boundary_message_id` text NOT NULL,
	`summary` text NOT NULL,
	`messages` integer NOT NULL,
	`tokens_before` integer NOT NULL,
	`tokens_after` integer NOT NULL,
	`provider_id` text NOT NULL,
	`model_id` text NOT NULL,
	`trigger` text NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`boundary_message_id`) REFERENCES `message`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK (`trigger` IN ('manual', 'auto')),
	CHECK (`messages` > 0)
);

CREATE INDEX `chat_compaction_conversation_idx` ON `chat_compaction` (`conversation_id`, `created_at`);
