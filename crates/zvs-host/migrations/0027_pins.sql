ALTER TABLE `conversation` ADD `pinned` integer DEFAULT false NOT NULL;
ALTER TABLE `message` ADD `pinned` integer DEFAULT false NOT NULL;
