CREATE TABLE `download` (
	`id` text PRIMARY KEY NOT NULL,
	`item_kind` text NOT NULL,
	`item_ref` text NOT NULL,
	`display_name` text NOT NULL,
	`version` text,
	`size_bytes` integer DEFAULT 0 NOT NULL,
	`bytes_done` integer DEFAULT 0 NOT NULL,
	`status` text NOT NULL,
	`priority` integer DEFAULT 5 NOT NULL,
	`target_path` text NOT NULL,
	`url` text NOT NULL,
	`checksum_algorithm` text,
	`checksum_value` text,
	`error` text,
	`run_id` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	`started_at` integer,
	`finished_at` integer
);
--> statement-breakpoint
CREATE INDEX `download_status_idx` ON `download` (`status`);--> statement-breakpoint
CREATE INDEX `download_item_ref_idx` ON `download` (`item_ref`);