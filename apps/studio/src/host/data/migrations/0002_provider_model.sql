CREATE TABLE `provider` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`name` text NOT NULL,
	`base_url` text NOT NULL,
	`secret_id` text,
	`capabilities` text NOT NULL,
	`cap_text` integer DEFAULT false NOT NULL,
	`cap_embedding` integer DEFAULT false NOT NULL,
	`cap_image` integer DEFAULT false NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`settings` text DEFAULT '{}' NOT NULL,
	`default_model_id` text,
	`status` text DEFAULT 'unknown' NOT NULL,
	`status_detail` text,
	`last_probe_at` integer,
	`last_latency_ms` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`secret_id`) REFERENCES `secret`(`id`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE UNIQUE INDEX `provider_kind_name_unq` ON `provider` (`kind`,`name`);--> statement-breakpoint
CREATE INDEX `provider_cap_text_idx` ON `provider` (`cap_text`);--> statement-breakpoint
CREATE INDEX `provider_cap_embedding_idx` ON `provider` (`cap_embedding`);--> statement-breakpoint
CREATE INDEX `provider_cap_image_idx` ON `provider` (`cap_image`);--> statement-breakpoint
CREATE TABLE `model` (
	`id` text PRIMARY KEY NOT NULL,
	`provider_id` text NOT NULL,
	`external_id` text NOT NULL,
	`display_name` text NOT NULL,
	`family` text,
	`context_window` integer,
	`max_output` integer,
	`size_bytes` integer,
	`capabilities` text DEFAULT '[]' NOT NULL,
	`available` integer DEFAULT true NOT NULL,
	`unavailable_reason` text,
	`discovered_at` integer NOT NULL,
	FOREIGN KEY (`provider_id`) REFERENCES `provider`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `model_provider_external_unq` ON `model` (`provider_id`,`external_id`);