CREATE TABLE IF NOT EXISTS plugins (
    id BLOB PRIMARY KEY NOT NULL,
    name TEXT NOT NULL,
    version TEXT NOT NULL,
    display_name TEXT NOT NULL,
    description TEXT NOT NULL,
    skills TEXT NOT NULL,
    tools TEXT NOT NULL,
    permissions TEXT NOT NULL,
    enabled INTEGER NOT NULL DEFAULT 1,
    installed_at TEXT NOT NULL
);

CREATE INDEX IF NOT EXISTS idx_plugins_name ON plugins(name);
