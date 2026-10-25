CREATE TABLE `agentic_session` (
	`id` text PRIMARY KEY NOT NULL,
	`goal` text NOT NULL,
	`status` text NOT NULL,
	`stop_reason` text,
	`error` text,
	`summary` text,
	`mode` text NOT NULL,
	`pending_mode` text,
	`workspace` text NOT NULL,
	`provider_id` text NOT NULL,
	`model_id` text NOT NULL,
	`tools` text DEFAULT '[]' NOT NULL,
	`budget` text NOT NULL,
	`usage` text NOT NULL,
	`plan` text DEFAULT '[]' NOT NULL,
	`pending_plan` text,
	`pending_instructions` text DEFAULT '[]' NOT NULL,
	`changed_files` text DEFAULT '[]' NOT NULL,
	`transcript` text DEFAULT '[]' NOT NULL,
	`run_id` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE INDEX `agentic_session_updated_idx` ON `agentic_session` (`updated_at`);
