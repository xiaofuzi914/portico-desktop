CREATE TABLE IF NOT EXISTS skills (
    id BLOB PRIMARY KEY NOT NULL,
    plugin_id BLOB NOT NULL,
    name TEXT NOT NULL,
    description TEXT NOT NULL,
    trigger_description TEXT NOT NULL,
    instruction_file TEXT,
    required_tools TEXT NOT NULL,
    FOREIGN KEY (plugin_id) REFERENCES plugins(id) ON DELETE CASCADE
);

CREATE INDEX IF NOT EXISTS idx_skills_plugin_id ON skills(plugin_id);
CREATE INDEX IF NOT EXISTS idx_skills_name ON skills(name);
