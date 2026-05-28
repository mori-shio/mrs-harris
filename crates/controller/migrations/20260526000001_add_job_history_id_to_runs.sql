-- Add job_history_id column to job_runs
ALTER TABLE job_runs ADD COLUMN job_history_id BIGINT NULL;

-- Migrate existing version references to job_history_id
-- We map using job_id and config_version (fallback to version if config_version is null, and 1 if version is also weirdly null)
UPDATE job_runs r
JOIN job_history h ON r.job_id = h.job_id AND h.version = COALESCE(r.config_version, r.version, 1)
SET r.job_history_id = h.id;

-- For any orphans (should not happen with our seed data, but just in case), map to the latest history ID
UPDATE job_runs r
JOIN (
    SELECT job_id, MAX(id) as max_h_id FROM job_history GROUP BY job_id
) latest ON r.job_id = latest.job_id
SET r.job_history_id = latest.max_h_id
WHERE r.job_history_id IS NULL;

-- Drop redundant version columns
ALTER TABLE job_runs
    DROP COLUMN version,
    DROP COLUMN config_version;

-- Add foreign key constraint (if a history is deleted, the run just loses its history mapping but keeps existing)
-- Alternatively, ON DELETE CASCADE, but usually runs shouldn't be deleted if history is deleted.
ALTER TABLE job_runs 
    ADD CONSTRAINT fk_job_runs_history FOREIGN KEY (job_history_id) REFERENCES job_history(id) ON DELETE SET NULL;
