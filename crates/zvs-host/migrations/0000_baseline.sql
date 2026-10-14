CREATE TABLE `setting` (
	`key` text PRIMARY KEY NOT NULL,
	`value` text NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE TABLE `secret` (
	`id` text PRIMARY KEY NOT NULL,
	`type` text NOT NULL,
	`name` text NOT NULL,
	`scope` text NOT NULL,
	`cipher` blob,
	`cipher_version` integer DEFAULT 1 NOT NULL,
	`hint` text,
	`fields` text DEFAULT '{}' NOT NULL,
	`tags` text DEFAULT '[]' NOT NULL,
	`note` text,
	`rotation_days` integer,
	`rotates_at` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE INDEX `secret_type_idx` ON `secret` (`type`);

CREATE INDEX `secret_scope_idx` ON `secret` (`scope`);

CREATE UNIQUE INDEX `secret_name_type_unq` ON `secret` (`name`,`type`);

CREATE TABLE `secret_usage` (
	`secret_id` text NOT NULL,
	`consumer_kind` text NOT NULL,
	`consumer_id` text NOT NULL,
	`created_at` integer NOT NULL,
	PRIMARY KEY(`secret_id`, `consumer_kind`, `consumer_id`),
	FOREIGN KEY (`secret_id`) REFERENCES `secret`(`id`) ON UPDATE cascade ON DELETE restrict
);

CREATE TABLE `provider` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`name` text NOT NULL,
	`base_url` text NOT NULL,
	`secret_id` text,
	`capabilities` text NOT NULL,
	`cap_text` integer DEFAULT false NOT NULL,
	`cap_embedding` integer DEFAULT false NOT NULL,
	`cap_image` integer DEFAULT false NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`settings` text DEFAULT '{}' NOT NULL,
	`default_model_id` text,
	`status` text DEFAULT 'unknown' NOT NULL,
	`status_detail` text,
	`last_probe_at` integer,
	`last_latency_ms` integer,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL, `adapter` text DEFAULT 'openai-compatible' NOT NULL, `auth_mode` text DEFAULT 'api' NOT NULL, `account_id` text REFERENCES account(id),
	FOREIGN KEY (`secret_id`) REFERENCES `secret`(`id`) ON UPDATE no action ON DELETE restrict
);

CREATE UNIQUE INDEX `provider_kind_name_unq` ON `provider` (`kind`,`name`);

CREATE INDEX `provider_cap_text_idx` ON `provider` (`cap_text`);

CREATE INDEX `provider_cap_embedding_idx` ON `provider` (`cap_embedding`);

CREATE INDEX `provider_cap_image_idx` ON `provider` (`cap_image`);

CREATE TABLE `model` (
	`id` text PRIMARY KEY NOT NULL,
	`provider_id` text NOT NULL,
	`external_id` text NOT NULL,
	`display_name` text NOT NULL,
	`family` text,
	`context_window` integer,
	`max_output` integer,
	`size_bytes` integer,
	`capabilities` text DEFAULT '[]' NOT NULL,
	`available` integer DEFAULT true NOT NULL,
	`unavailable_reason` text,
	`discovered_at` integer NOT NULL, `is_free` integer, `no_training` integer,
	FOREIGN KEY (`provider_id`) REFERENCES `provider`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `model_provider_external_unq` ON `model` (`provider_id`,`external_id`);

CREATE TABLE `account` (
	`id` text PRIMARY KEY NOT NULL,
	`adapter` text NOT NULL,
	`partition` text NOT NULL,
	`external_id` text NOT NULL,
	`email_masked` text,
	`display_name` text,
	`avatar_url` text,
	`token_secret_id` text,
	`token_expires_at` integer,
	`status` text DEFAULT 'linked' NOT NULL,
	`status_detail` text,
	`last_checked_at` integer,
	`linked_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	FOREIGN KEY (`token_secret_id`) REFERENCES `secret`(`id`) ON UPDATE no action ON DELETE set null
);

CREATE UNIQUE INDEX `account_adapter_external_unq` ON `account` (`adapter`,`external_id`);

CREATE INDEX `account_adapter_idx` ON `account` (`adapter`);

CREATE TABLE `vector_store` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`backend` text DEFAULT 'lancedb' NOT NULL,
	`embedding_provider_id` text NOT NULL,
	`embedding_model_id` text NOT NULL,
	`dimension` integer NOT NULL,
	`metric` text NOT NULL,
	`chunk_size` integer NOT NULL,
	`chunk_overlap` integer NOT NULL,
	`index_type` text DEFAULT 'FLAT' NOT NULL,
	`status` text DEFAULT 'pending' NOT NULL,
	`table_created_at` integer,
	`last_indexed_at` integer,
	`documents` integer DEFAULT 0 NOT NULL,
	`vectors` integer DEFAULT 0 NOT NULL,
	`bytes` integer DEFAULT 0 NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL, `rerank` text DEFAULT '{"enabled":false,"modelRef":"","candidates":50}' NOT NULL, `ocr` text DEFAULT '{"enabled":false,"modelRef":"","language":"auto","minCharsPerPage":200}' NOT NULL, `graph_stale` integer DEFAULT false NOT NULL, `last_eval` text, `query_prefix` text DEFAULT '' NOT NULL, `document_prefix` text DEFAULT '' NOT NULL, `search_mode` text DEFAULT 'hybrid' NOT NULL, `schema_version` integer DEFAULT 1 NOT NULL, `chat_min_score` real DEFAULT 0.3 NOT NULL,
	FOREIGN KEY (`embedding_provider_id`) REFERENCES `provider`(`id`) ON UPDATE no action ON DELETE restrict
);

CREATE UNIQUE INDEX `vector_store_name_unq` ON `vector_store` (`name`);

CREATE TABLE `vector_document` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`source_path` text NOT NULL,
	`content_hash` text NOT NULL,
	`chunk_count` integer NOT NULL,
	`bytes` integer NOT NULL,
	`indexed_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `vector_document_store_path_unq` ON `vector_document` (`store_id`,`source_path`);

CREATE TABLE `run` (
	`id` text PRIMARY KEY NOT NULL,
	`kind` text NOT NULL,
	`subject_id` text,
	`status` text NOT NULL,
	`graph` text NOT NULL,
	`input` text,
	`outcome` text,
	`stream_id` text NOT NULL,
	`concurrency` integer NOT NULL,
	`created_at` integer NOT NULL,
	`started_at` integer,
	`finished_at` integer,
	`error` text
, `title` text, `retry_of_id` text, `pruned_at` integer);

CREATE UNIQUE INDEX `run_stream_id_unique` ON `run` (`stream_id`);

CREATE INDEX `run_status_idx` ON `run` (`status`);

CREATE TABLE `step` (
	`id` text PRIMARY KEY NOT NULL,
	`run_id` text NOT NULL,
	`node_id` text NOT NULL,
	`type` text NOT NULL,
	`status` text NOT NULL,
	`input` text,
	`output` text,
	`error` text,
	`started_at` integer NOT NULL,
	`finished_at` integer,
	`attempt` integer NOT NULL,
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `step_attempt_unq` ON `step` (`run_id`,`node_id`,`attempt`);

CREATE TABLE `run_event` (
	`run_id` text NOT NULL,
	`seq` integer NOT NULL,
	`event` text NOT NULL,
	PRIMARY KEY(`run_id`, `seq`),
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE TABLE `pending_approval` (
	`id` text PRIMARY KEY NOT NULL,
	`run_id` text NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`expires_at` integer NOT NULL,
	`decision` text
);

CREATE TABLE `permission_use` (
	`id` text PRIMARY KEY NOT NULL,
	`grant_id` text NOT NULL,
	`run_id` text NOT NULL,
	`used_at` integer NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`tier` text NOT NULL,
	`granted_by` text NOT NULL,
	`granted_at` integer NOT NULL
);

CREATE TABLE `tool_permission` (
	`id` text PRIMARY KEY NOT NULL,
	`subject` text NOT NULL,
	`scope` text NOT NULL,
	`tier` text NOT NULL,
	`granted_at` integer NOT NULL,
	`granted_by` text NOT NULL
);

CREATE UNIQUE INDEX `tool_permission_subject_scope` ON `tool_permission` (`subject`,`scope`);

CREATE TABLE `conversation` (
	`id` text PRIMARY KEY NOT NULL,
	`title` text NOT NULL,
	`provider_id` text NOT NULL,
	`model_id` text NOT NULL,
	`settings` text NOT NULL,
	`system_prompt` text NOT NULL,
	`attached_store_ids` text NOT NULL,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL
, `use_relations` integer DEFAULT false NOT NULL, `enabled_tools` text DEFAULT '[]' NOT NULL);

CREATE TABLE `message` (
	`id` text PRIMARY KEY NOT NULL,
	`conversation_id` text NOT NULL,
	`role` text NOT NULL,
	`content` text NOT NULL,
	`citations` text NOT NULL,
	`tokens_in` integer NOT NULL,
	`tokens_out` integer NOT NULL,
	`usage_estimated` integer DEFAULT true NOT NULL,
	`duration_ms` integer NOT NULL,
	`run_id` text,
	`partial` integer DEFAULT false NOT NULL,
	`created_at` integer NOT NULL, `reasoning` text DEFAULT '' NOT NULL, `tool_calls` text DEFAULT '[]' NOT NULL,
	FOREIGN KEY (`conversation_id`) REFERENCES `conversation`(`id`) ON UPDATE no action ON DELETE cascade,
	FOREIGN KEY (`run_id`) REFERENCES `run`(`id`) ON UPDATE no action ON DELETE set null
);

CREATE INDEX `message_conversation_created_idx` ON `message` (`conversation_id`,`created_at`,`id`);

CREATE INDEX `run_created_idx` ON `run` (`created_at`,`id`);

CREATE INDEX `run_finished_idx` ON `run` (`finished_at`);

CREATE TABLE `vector_source` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`kind` text NOT NULL,
	`path` text NOT NULL,
	`include` text NOT NULL,
	`exclude` text NOT NULL,
	`recursive` integer DEFAULT true NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `vector_source_store_path_unq` ON `vector_source` (`store_id`,`path`);

CREATE TABLE `download` (
	`id` text PRIMARY KEY NOT NULL,
	`item_kind` text NOT NULL,
	`item_ref` text NOT NULL,
	`display_name` text NOT NULL,
	`version` text,
	`size_bytes` integer DEFAULT 0 NOT NULL,
	`bytes_done` integer DEFAULT 0 NOT NULL,
	`status` text NOT NULL,
	`priority` integer DEFAULT 5 NOT NULL,
	`target_path` text NOT NULL,
	`url` text NOT NULL,
	`checksum_algorithm` text,
	`checksum_value` text,
	`error` text,
	`run_id` text,
	`created_at` integer NOT NULL,
	`updated_at` integer NOT NULL,
	`started_at` integer,
	`finished_at` integer
);

CREATE INDEX `download_status_idx` ON `download` (`status`);

CREATE INDEX `download_item_ref_idx` ON `download` (`item_ref`);

CREATE TABLE `graph_relation_type` (
	`id` text PRIMARY KEY NOT NULL,
	`name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`directed` integer DEFAULT true NOT NULL
);

CREATE UNIQUE INDEX `graph_relation_type_name_unq` ON `graph_relation_type` (`name`);

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

CREATE UNIQUE INDEX `graph_edge_unq` ON `graph_edge` (`store_id`,`from_document_id`,`to_document_id`,`relation_type_id`);

CREATE TABLE `tool` (
	`id` text PRIMARY KEY NOT NULL,
	`key` text NOT NULL,
	`source` text NOT NULL,
	`source_ref` text,
	`display_name` text NOT NULL,
	`description` text DEFAULT '' NOT NULL,
	`input_schema` text,
	`read_only` integer DEFAULT false NOT NULL,
	`destructive` integer DEFAULT false NOT NULL,
	`tier` text NOT NULL,
	`enabled` integer DEFAULT true NOT NULL,
	`available` integer DEFAULT true NOT NULL,
	`last_used_at` integer,
	`call_count` integer DEFAULT 0 NOT NULL,
	`registered_at` integer NOT NULL,
	`updated_at` integer NOT NULL
);

CREATE UNIQUE INDEX `tool_key_unq` ON `tool` (`key`);

CREATE TABLE `tool_reference` (
	`id` text PRIMARY KEY NOT NULL,
	`tool_id` text NOT NULL,
	`kind` text NOT NULL,
	`ref_id` text NOT NULL,
	`name` text NOT NULL,
	FOREIGN KEY (`tool_id`) REFERENCES `tool`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE UNIQUE INDEX `tool_reference_unq` ON `tool_reference` (`tool_id`,`kind`,`ref_id`);

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

CREATE UNIQUE INDEX `mcp_capability_unq` ON `mcp_capability` (`server_id`,`kind`,`name`);

CREATE INDEX `mcp_capability_tool_key_idx` ON `mcp_capability` (`tool_key`);

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

CREATE UNIQUE INDEX `mcp_server_name_unq` ON `mcp_server` (`name`);

CREATE UNIQUE INDEX `mcp_server_namespace_unq` ON `mcp_server` (`namespace`);

CREATE INDEX `mcp_server_startup_idx` ON `mcp_server` (`startup`);

CREATE TABLE `vector_eval_case` (
	`id` text PRIMARY KEY NOT NULL,
	`store_id` text NOT NULL,
	`question` text NOT NULL,
	`expected_document_ids` text NOT NULL,
	`created_at` integer NOT NULL,
	FOREIGN KEY (`store_id`) REFERENCES `vector_store`(`id`) ON UPDATE no action ON DELETE cascade
);

CREATE INDEX `vector_eval_case_store_idx` ON `vector_eval_case` (`store_id`,`created_at`);
