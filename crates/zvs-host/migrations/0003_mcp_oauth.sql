CREATE TABLE `mcp_oauth` (
	`server_id` text PRIMARY KEY NOT NULL,
	`cipher` blob NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`server_id`) REFERENCES `mcp_server`(`id`) ON UPDATE no action ON DELETE cascade
);
