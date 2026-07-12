ALTER TABLE plugins ADD COLUMN entrypoint TEXT;
ALTER TABLE plugins ADD COLUMN capabilities TEXT NOT NULL DEFAULT '[]';
ALTER TABLE plugins ADD COLUMN install_path TEXT;
