ALTER TABLE `conversation` ADD `enabled_tools` text DEFAULT '[]' NOT NULL;--> statement-breakpoint
ALTER TABLE `message` ADD `tool_calls` text DEFAULT '[]' NOT NULL;