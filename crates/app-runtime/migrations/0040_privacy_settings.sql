-- Local product settings for privacy / learning controls.
CREATE TABLE IF NOT EXISTS app_settings (
    key TEXT PRIMARY KEY NOT NULL,
    value_json TEXT NOT NULL,
    updated_at DATETIME NOT NULL
);
