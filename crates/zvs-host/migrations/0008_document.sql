CREATE TABLE `document` (
	`id` text PRIMARY KEY NOT NULL,
	`content_hash` text NOT NULL,
	`source_path` text NOT NULL,
	`kind` text NOT NULL,
	`mime_type` text NOT NULL,
	`page_count` integer,
	`part_count` integer NOT NULL,
	`size_bytes` integer NOT NULL,
	`cached_file` text NOT NULL,
	`opened_at` integer NOT NULL
);

CREATE UNIQUE INDEX `document_content_hash_unq` ON `document` (`content_hash`);

CREATE INDEX `document_opened_at_idx` ON `document` (`opened_at`);
