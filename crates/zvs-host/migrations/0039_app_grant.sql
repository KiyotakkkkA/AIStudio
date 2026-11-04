CREATE TABLE `app_grant` (
	`id` text PRIMARY KEY NOT NULL,
	`app` text NOT NULL,
	`path` text NOT NULL,
	`icon` text,
	`scope` text NOT NULL,
	`conversation_id` text,
	`granted_at` integer NOT NULL,
	`last_used_at` integer,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK ((`scope` = 'always' AND `conversation_id` IS NULL) OR (`scope` = 'conversation' AND `conversation_id` IS NOT NULL))
);

CREATE INDEX `app_grant_conversation_idx` ON `app_grant` (`conversation_id`);
CREATE INDEX `app_grant_app_idx` ON `app_grant` (`app`);
