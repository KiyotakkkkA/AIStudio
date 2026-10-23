CREATE TABLE `scenario_draft` (
	`scenario_id` text PRIMARY KEY NOT NULL,
	`graph` text NOT NULL,
	`saved_at` integer NOT NULL,
	FOREIGN KEY (`scenario_id`) REFERENCES `scenario`(`id`) ON UPDATE no action ON DELETE cascade
);
