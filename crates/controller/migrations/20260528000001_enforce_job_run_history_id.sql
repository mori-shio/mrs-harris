-- Ensure every job run is permanently tied to the job configuration history
-- that existed when the run was created.

UPDATE job_runs r
JOIN (
    SELECT job_id, MAX(id) AS max_h_id
    FROM job_history
    GROUP BY job_id
) latest ON r.job_id = latest.job_id
SET r.job_history_id = latest.max_h_id
WHERE r.job_history_id IS NULL;

ALTER TABLE job_runs
    DROP FOREIGN KEY fk_job_runs_history;

ALTER TABLE job_runs
    MODIFY COLUMN job_history_id BIGINT NOT NULL;

ALTER TABLE job_runs
    ADD CONSTRAINT fk_job_runs_history
    FOREIGN KEY (job_history_id) REFERENCES job_history(id)
    ON DELETE RESTRICT;
