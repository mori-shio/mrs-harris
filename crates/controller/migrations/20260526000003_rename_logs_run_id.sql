-- Rename run_id column to job_run_id in job_logs table
-- Since the parent table is job_runs, this foreign key name aligns with self-documenting naming conventions.

ALTER TABLE job_logs RENAME COLUMN run_id TO job_run_id;
