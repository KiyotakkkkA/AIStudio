CREATE TABLE `access_grant` (
	`id` text PRIMARY KEY NOT NULL,
	`resource_kind` text NOT NULL,
	`resource` text NOT NULL,
	`action` text NOT NULL,
	`decision` text NOT NULL,
	`scope` text NOT NULL,
	`owner` text,
	`granted_by` text NOT NULL,
	`granted_at` integer NOT NULL,
	`used_at` integer,
	`detail` text,
	FOREIGN KEY (`owner`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	CHECK (`resource_kind` IN ('folder', 'site', 'app', 'tool')),
	CHECK (`decision` IN ('allow', 'ask', 'block')),
	CHECK ((`scope` = 'always' AND `owner` IS NULL) OR (`scope` = 'conversation' AND `owner` IS NOT NULL))
);

CREATE UNIQUE INDEX `access_grant_shared_idx` ON `access_grant` (`resource_kind`, `resource`, `action`) WHERE `owner` IS NULL;
CREATE UNIQUE INDEX `access_grant_owned_idx` ON `access_grant` (`resource_kind`, `resource`, `action`, `owner`) WHERE `owner` IS NOT NULL;
CREATE INDEX `access_grant_owner_idx` ON `access_grant` (`owner`);

INSERT OR IGNORE INTO `access_grant` (`id`, `resource_kind`, `resource`, `action`, `decision`, `scope`, `owner`, `granted_by`, `granted_at`, `used_at`, `detail`)
SELECT `id`, 'tool', `subject`, `scope`, CASE `tier` WHEN 'auto' THEN 'allow' WHEN 'off' THEN 'block' ELSE 'ask' END, 'always', NULL, `granted_by`, `granted_at`, NULL, NULL
FROM `tool_permission`;

INSERT OR IGNORE INTO `access_grant` (`id`, `resource_kind`, `resource`, `action`, `decision`, `scope`, `owner`, `granted_by`, `granted_at`, `used_at`, `detail`)
SELECT `id`, 'folder', `path`, '', 'allow', `scope`, `conversation_id`, 'user', `granted_at`, `last_used_at`, NULL
FROM `folder_grant`
ORDER BY `granted_at`, `id`;

INSERT OR IGNORE INTO `access_grant` (`id`, `resource_kind`, `resource`, `action`, `decision`, `scope`, `owner`, `granted_by`, `granted_at`, `used_at`, `detail`)
SELECT `id`, 'app', `app`, '', 'allow', `scope`, `conversation_id`, 'user', `granted_at`, `last_used_at`, json_object('path', `path`, 'icon', `icon`)
FROM `app_grant`
ORDER BY `granted_at` DESC, `id`;

INSERT OR IGNORE INTO `access_grant` (`id`, `resource_kind`, `resource`, `action`, `decision`, `scope`, `owner`, `granted_by`, `granted_at`, `used_at`, `detail`)
SELECT `id`, 'site', `origin`, `action`, `decision`, 'always', NULL, 'user', `updated_at`, NULL, NULL
FROM `browser_site_rule`;

DROP TABLE `tool_permission`;

DROP TABLE `folder_grant`;

DROP TABLE `app_grant`;

DROP TABLE `browser_site_rule`;
