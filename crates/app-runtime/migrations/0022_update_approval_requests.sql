-- Expand approval_requests with workspace/thread context, explicit status,
-- and a resolution reason.

ALTER TABLE approval_requests ADD COLUMN workspace_id BLOB;
ALTER TABLE approval_requests ADD COLUMN thread_id BLOB;
ALTER TABLE approval_requests ADD COLUMN status TEXT NOT NULL DEFAULT 'Pending';
ALTER TABLE approval_requests ADD COLUMN resolution_reason TEXT;

-- Legacy rows may have a decision column value; migrate them into status.
UPDATE approval_requests SET status = 'Approved' WHERE decision = 'approved';
UPDATE approval_requests SET status = 'Denied' WHERE decision = 'denied';

DROP INDEX IF EXISTS idx_approval_requests_run_id;
CREATE INDEX IF NOT EXISTS idx_approval_requests_run_id ON approval_requests(run_id);
CREATE INDEX IF NOT EXISTS idx_approval_requests_status ON approval_requests(status);
