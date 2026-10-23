CREATE TABLE `memory` (
	`id` text PRIMARY KEY NOT NULL,
	`scope` text NOT NULL,
	`conversation_id` text,
	`content` text NOT NULL,
	`source` text NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK ((`scope` = 'account' AND `conversation_id` IS NULL) OR (`scope` = 'conversation' AND `conversation_id` IS NOT NULL))
);

CREATE INDEX `memory_conversation_idx` ON `memory` (`conversation_id`);
