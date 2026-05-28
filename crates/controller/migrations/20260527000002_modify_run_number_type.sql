-- Modify run_number column to BIGINT to be compatible with Rust's i64 in SQLx
ALTER TABLE job_runs MODIFY COLUMN run_number BIGINT NOT NULL;
