CREATE TABLE `vector_eval_case` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`question` text NOT NULL,
	`expected_document_ids` text NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE INDEX `vector_eval_case_store_idx` ON `vector_eval_case` (`store_id`,`created_at`);--> statement-breakpoint
ALTER TABLE `vector_store` ADD `last_eval` text;