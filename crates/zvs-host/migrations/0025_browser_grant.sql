CREATE TABLE `browser_grant` (
	`id` text PRIMARY KEY NOT NULL,
	`origin` text NOT NULL,
	`capability` text NOT NULL,
	`granted_at` integer NOT NULL,
	CHECK (`capability` IN ('browser.click', 'browser.type'))
);

CREATE UNIQUE INDEX `browser_grant_origin_capability_idx` ON `browser_grant` (`origin`, `capability`);
