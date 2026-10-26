CREATE TABLE `message_tool_call` (
	`message_id` text NOT NULL,
	`idx` integer NOT NULL,
	`call` text NOT NULL,
	PRIMARY KEY(`message_id`, `idx`),
	FOREIGN KEY (`message_id`) REFERENCES `message`(`id`) ON UPDATE no action ON DELETE cascade
) WITHOUT ROWID;

INSERT INTO `message_tool_call` (`message_id`, `idx`, `call`)
SELECT `message`.`id`, CAST(`e`.`key` AS integer), json(`e`.`value`)
FROM `message`, json_each(`message`.`tool_calls`) AS `e`;

ALTER TABLE `message` DROP COLUMN `tool_calls`;

CREATE TABLE `agentic_transcript` (
	`session_id` text NOT NULL,
	`seq` integer NOT NULL,
	`entry` text NOT NULL,
	PRIMARY KEY(`session_id`, `seq`),
	FOREIGN KEY (`session_id`) REFERENCES `agentic_session`(`id`) ON UPDATE no action ON DELETE cascade
) WITHOUT ROWID;

INSERT INTO `agentic_transcript` (`session_id`, `seq`, `entry`)
SELECT `agentic_session`.`id`, CAST(`e`.`key` AS integer), json(`e`.`value`)
FROM `agentic_session`, json_each(`agentic_session`.`transcript`) AS `e`;

ALTER TABLE `agentic_session` DROP COLUMN `transcript`;
