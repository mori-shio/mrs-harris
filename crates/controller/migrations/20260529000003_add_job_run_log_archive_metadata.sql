ALTER TABLE job_runs
    ADD COLUMN log_archive_status VARCHAR(32) NULL AFTER duration_ms,
    ADD COLUMN log_archive_store VARCHAR(32) NULL AFTER log_archive_status,
    ADD COLUMN log_archive_key VARCHAR(1024) NULL AFTER log_archive_store,
    ADD COLUMN log_line_count BIGINT NULL AFTER log_archive_key,
    ADD COLUMN log_archive_bytes BIGINT NULL AFTER log_line_count,
    ADD COLUMN log_archived_at TIMESTAMP(3) NULL AFTER log_archive_bytes;

CREATE INDEX idx_job_runs_log_archive_status ON job_runs (log_archive_status);
