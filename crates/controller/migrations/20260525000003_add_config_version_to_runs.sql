-- Add config_version column to job_runs
ALTER TABLE job_runs ADD COLUMN config_version INT UNSIGNED NULL;
