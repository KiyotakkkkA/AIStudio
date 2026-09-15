CREATE TABLE `graph_relation_type` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`directed` integer DEFAULT true NOT NULL
);
--> statement-breakpoint
CREATE UNIQUE INDEX `graph_relation_type_name_unq` ON `graph_relation_type` (`name`);--> statement-breakpoint
CREATE TABLE `graph_edge` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`from_document_id` text NOT NULL,
	`to_document_id` text NOT NULL,
	`relation_type_id` text NOT NULL,
	`weight` real,
	`metadata` text,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`from_document_id`) REFERENCES `vector_document`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`to_document_id`) REFERENCES `vector_document`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`relation_type_id`) REFERENCES `graph_relation_type`(`id`) ON UPDATE no action ON DELETE restrict
);
--> statement-breakpoint
CREATE UNIQUE INDEX `graph_edge_unq` ON `graph_edge` (`store_id`,`from_document_id`,`to_document_id`,`relation_type_id`);--> statement-breakpoint
ALTER TABLE `vector_store` ADD `graph_stale` integer DEFAULT false NOT NULL;
--> statement-breakpoint
INSERT INTO `graph_relation_type` (`id`, `name`, `description`, `directed`) VALUES
('01999a00-0001-7000-8000-000000000001','cites','Документ ссылается на другой документ',1),
('01999a00-0002-7000-8000-000000000002','derived_from','Документ получен из другого документа',1),
('01999a00-0003-7000-8000-000000000003','co_occurs','Документы встречаются вместе',0),
('01999a00-0004-7000-8000-000000000004','user_linked','Связь, проведённая пользователем вручную',1);
