ALTER TABLE `vector_store` ADD `query_prefix` text DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE `vector_store` ADD `document_prefix` text DEFAULT '' NOT NULL;--> statement-breakpoint
ALTER TABLE `vector_store` ADD `search_mode` text DEFAULT 'hybrid' NOT NULL;--> statement-breakpoint
ALTER TABLE `vector_store` ADD `schema_version` integer DEFAULT 1 NOT NULL;--> statement-breakpoint
ALTER TABLE `conversation` ADD `use_relations` integer DEFAULT false NOT NULL;