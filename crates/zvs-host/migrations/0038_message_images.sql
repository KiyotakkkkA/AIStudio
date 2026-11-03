ALTER TABLE message ADD COLUMN images TEXT NOT NULL DEFAULT '[]' CHECK(json_valid(images));
