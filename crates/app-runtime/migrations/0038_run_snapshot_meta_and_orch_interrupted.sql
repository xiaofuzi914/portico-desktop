-- Persist model-selection metadata for multi-agent tier/thinking observability.
ALTER TABLE run_model_snapshots ADD COLUMN selection_reason TEXT;
ALTER TABLE run_model_snapshots ADD COLUMN thinking_mode TEXT;
ALTER TABLE run_model_snapshots ADD COLUMN thinking_degraded INTEGER NOT NULL DEFAULT 0;
