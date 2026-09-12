CREATE TABLE `run` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`subject_id` text,
	`status` text NOT NULL,
	`graph` text NOT NULL,
	`input` text,
	`outcome` text,
	`stream_id` text NOT NULL,
	`concurrency` integer NOT NULL,
	`created_at` integer NOT NULL,
	`started_at` integer,
	`finished_at` integer,
	`error` text
);
--> statement-breakpoint
CREATE UNIQUE INDEX `run_stream_id_unique` ON `run` (`stream_id`);--> statement-breakpoint
CREATE INDEX `run_status_idx` ON `run` (`status`);--> statement-breakpoint
CREATE TABLE `step` (
	`id` text PRIMARY KEY NOT NULL,
	`run_id` text NOT NULL,
	`node_id` text NOT NULL,
	`type` text NOT NULL,
	`status` text NOT NULL,
	`input` text,
	`output` text,
	`error` text,
	`started_at` integer NOT NULL,
	`finished_at` integer,
	`attempt` integer NOT NULL,
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `step_attempt_unq` ON `step` (`run_id`,`node_id`,`attempt`);--> statement-breakpoint
CREATE TABLE `run_event` (
	`run_id` text NOT NULL,
	`seq` integer NOT NULL,
	`event` text NOT NULL,
	PRIMARY KEY(`run_id`, `seq`),
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE cascade
);
