-- Remove redundant worker_type column from jobs and job_runs tables
-- Since worker_definition_id has already been introduced and references worker_definitions(id),
-- the worker_type (fargate / lambda) is fully defined by the worker definition itself.

-- Ensure every record has a valid worker_definition_id (just in case)
UPDATE jobs SET worker_definition_id = 1 WHERE worker_definition_id IS NULL;
UPDATE job_runs SET worker_definition_id = 1 WHERE worker_definition_id IS NULL;

-- Drop redundant worker_type columns
ALTER TABLE jobs DROP COLUMN worker_type;
ALTER TABLE job_runs DROP COLUMN worker_type;
