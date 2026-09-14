CREATE TABLE `vector_source` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`kind` text NOT NULL,
	`path` text NOT NULL,
	`include` text NOT NULL,
	`exclude` text NOT NULL,
	`recursive` integer DEFAULT true NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `vector_source_store_path_unq` ON `vector_source` (`store_id`,`path`);