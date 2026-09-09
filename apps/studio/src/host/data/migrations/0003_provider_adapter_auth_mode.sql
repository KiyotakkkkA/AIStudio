ALTER TABLE `provider` ADD `adapter` text DEFAULT 'openai-compatible' NOT NULL;--> statement-breakpoint
ALTER TABLE `provider` ADD `auth_mode` text DEFAULT 'api' NOT NULL;--> statement-breakpoint
UPDATE `provider` SET `adapter` = 'anthropic' WHERE `kind` = 'anthropic';