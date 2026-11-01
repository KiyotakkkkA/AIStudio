ALTER TABLE `run` ADD `parent_run_id` text;

CREATE INDEX `run_parent_idx` ON `run` (`parent_run_id`);
