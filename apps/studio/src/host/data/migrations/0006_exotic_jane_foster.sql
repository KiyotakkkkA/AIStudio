CREATE TABLE `vector_store` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`backend` text DEFAULT 'lancedb' NOT NULL,
	`embedding_provider_id` text NOT NULL,
	`embedding_model_id` text NOT NULL,
	`dimension` integer NOT NULL,
	`metric` text NOT NULL,
	`chunk_size` integer NOT NULL,
	`chunk_overlap` integer NOT NULL,
	`index_type` text DEFAULT 'FLAT' NOT NULL,
	`status` text DEFAULT 'pending' NOT NULL,
	`table_created_at` integer,
	`last_indexed_at` integer,
	`documents` integer DEFAULT 0 NOT NULL,
	`vectors` integer DEFAULT 0 NOT NULL,
	`bytes` integer DEFAULT 0 NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`embedding_provider_id`) REFERENCES `provider`(`id`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE UNIQUE INDEX `vector_store_name_unq` ON `vector_store` (`name`);--> statement-breakpoint
CREATE TABLE `vector_document` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`source_path` text NOT NULL,
	`content_hash` text NOT NULL,
	`chunk_count` integer NOT NULL,
	`bytes` integer NOT NULL,
	`indexed_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `vector_document_store_path_unq` ON `vector_document` (`store_id`,`source_path`);