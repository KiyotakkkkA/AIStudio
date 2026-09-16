CREATE TABLE `mcp_capability` (
	`id` text PRIMARY KEY NOT NULL,
	`server_id` text NOT NULL,
	`kind` text NOT NULL,
	`name` text NOT NULL,
	`title` text,
	`description` text DEFAULT '' NOT NULL,
	`tool_key` text,
	`uri` text,
	`mime_type` text,
	`schema` text,
	`read_only` integer DEFAULT false NOT NULL,
	`destructive` integer DEFAULT false NOT NULL,
	`discovered_at` integer NOT NULL,
	FOREIGN KEY (`server_id`) REFERENCES `mcp_server`(`id`) ON UPDATE no action ON DELETE cascade
);
--> statement-breakpoint
CREATE UNIQUE INDEX `mcp_capability_unq` ON `mcp_capability` (`server_id`,`kind`,`name`);--> statement-breakpoint
CREATE INDEX `mcp_capability_tool_key_idx` ON `mcp_capability` (`tool_key`);--> statement-breakpoint
CREATE TABLE `mcp_server` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`namespace` text NOT NULL,
	`transport` text NOT NULL,
	`command` text,
	`args` text DEFAULT '[]' NOT NULL,
	`env` text DEFAULT '{}' NOT NULL,
	`url` text,
	`headers` text DEFAULT '{}' NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`startup` text DEFAULT 'manual' NOT NULL,
	`status` text DEFAULT 'stopped' NOT NULL,
	`protocol_version` text,
	`server_name` text,
	`server_version` text,
	`last_error` text,
	`last_connected_at` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `mcp_server_name_unq` ON `mcp_server` (`name`);--> statement-breakpoint
CREATE UNIQUE INDEX `mcp_server_namespace_unq` ON `mcp_server` (`namespace`);--> statement-breakpoint
CREATE INDEX `mcp_server_startup_idx` ON `mcp_server` (`startup`);