ALTER TABLE `run` ADD `title` text;--> statement-breakpoint
ALTER TABLE `run` ADD `retry_of_id` text;--> statement-breakpoint
ALTER TABLE `run` ADD `pruned_at` integer;--> statement-breakpoint
CREATE INDEX `run_created_idx` ON `run` (`created_at`,`id`);--> statement-breakpoint
CREATE INDEX `run_finished_idx` ON `run` (`finished_at`);