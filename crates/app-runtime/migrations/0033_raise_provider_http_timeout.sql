-- Remote chat-with-tools and long generations routinely exceed the old 30s
-- HTTP timeout (seen as "provider chat failed: request timed out"). Raise the
-- floor for existing providers still on the previous default.
UPDATE provider_configs
SET timeout_ms = 120000
WHERE timeout_ms IS NULL OR timeout_ms <= 30000;
