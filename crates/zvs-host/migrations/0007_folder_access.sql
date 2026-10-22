CREATE TABLE `folder_grant` (
	`id` text PRIMARY KEY NOT NULL,
	`path` text NOT NULL,
	`scope` text NOT NULL,
	`conversation_id` text,
	`granted_at` integer NOT NULL,
	`last_used_at` integer,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK ((`scope` = 'always' AND `conversation_id` IS NULL) OR (`scope` = 'conversation' AND `conversation_id` IS NOT NULL))
);

CREATE INDEX `folder_grant_conversation_idx` ON `folder_grant` (`conversation_id`);

ALTER TABLE `conversation` ADD `working_folder` text;

ALTER TABLE `conversation` ADD `turn_folder` text;
