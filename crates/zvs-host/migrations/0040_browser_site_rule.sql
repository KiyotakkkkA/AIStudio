CREATE TABLE `browser_site_rule` (
	`id` text PRIMARY KEY NOT NULL,
	`origin` text NOT NULL,
	`action` text NOT NULL,
	`decision` text NOT NULL,
	`updated_at` integer NOT NULL,
	CHECK (`action` IN ('browse', 'click', 'type', 'submit', 'download')),
	CHECK (`decision` IN ('ask', 'allow', 'block'))
);

CREATE UNIQUE INDEX `browser_site_rule_origin_action_idx` ON `browser_site_rule` (`origin`, `action`);

INSERT INTO `browser_site_rule` (`id`, `origin`, `action`, `decision`, `updated_at`)
SELECT `id`, `origin`, CASE `capability` WHEN 'browser.click' THEN 'click' ELSE 'type' END, 'allow', `granted_at`
FROM `browser_grant`;

DROP TABLE `browser_grant`;
