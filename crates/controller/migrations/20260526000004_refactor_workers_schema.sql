-- Refactor workers and job_runs schema for complete normalization
-- 1. Rename worker_tracking to workers
RENAME TABLE worker_tracking TO workers;

-- 2. Modify workers table: drop old fk, rename run_id, and make external_id nullable
ALTER TABLE workers DROP FOREIGN KEY workers_ibfk_1;
ALTER TABLE workers RENAME COLUMN run_id TO job_run_id;
ALTER TABLE workers MODIFY COLUMN external_id VARCHAR(512) NULL;

-- 3. Add worker_definition_id to workers (allowing NULL initially for migration)
ALTER TABLE workers ADD COLUMN worker_definition_id BIGINT NULL;

-- 4. Create workers records for all existing job_runs that have non-null string worker_ids
INSERT INTO workers (worker_definition_id, external_id, status, job_run_id, started_at, last_heartbeat, metadata)
SELECT 
    COALESCE(r.worker_definition_id, 1) as worker_definition_id,
    r.worker_id as external_id,
    CASE 
        WHEN r.status = 'running' THEN 'running'
        WHEN r.status = 'failed' THEN 'failed'
        ELSE 'completed'
    END as status,
    r.id as job_run_id,
    COALESCE(r.started_at, r.created_at) as started_at,
    r.finished_at as last_heartbeat,
    '{}' as metadata
FROM job_runs r
WHERE r.worker_id IS NOT NULL AND r.worker_id != '';

-- 5. Update job_runs.worker_id string to be the new workers.id
UPDATE job_runs r
JOIN workers w ON r.id = w.job_run_id AND r.worker_id = w.external_id
SET r.worker_id = w.id;

-- 6. Set any empty or non-numeric worker_id to NULL
UPDATE job_runs SET worker_id = NULL WHERE worker_id = '';
UPDATE job_runs SET worker_id = NULL WHERE worker_id IS NOT NULL AND worker_id NOT REGEXP '^[0-9]+$';

-- 7. Alter job_runs.worker_id column type to BIGINT
ALTER TABLE job_runs MODIFY COLUMN worker_id BIGINT NULL;

-- 8. Add foreign key constraints and indexes
ALTER TABLE workers MODIFY COLUMN worker_definition_id BIGINT NOT NULL;
ALTER TABLE workers ADD CONSTRAINT fk_workers_definition FOREIGN KEY (worker_definition_id) REFERENCES worker_definitions(id);
ALTER TABLE workers ADD CONSTRAINT fk_workers_job_run FOREIGN KEY (job_run_id) REFERENCES job_runs(id) ON DELETE CASCADE;

ALTER TABLE job_runs ADD CONSTRAINT fk_job_runs_worker FOREIGN KEY (worker_id) REFERENCES workers(id) ON DELETE SET NULL;

-- 9. Drop the redundant worker_definition_id column from job_runs
ALTER TABLE job_runs DROP COLUMN worker_definition_id;
