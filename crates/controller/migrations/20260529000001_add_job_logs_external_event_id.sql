ALTER TABLE job_logs
    ADD COLUMN external_event_id VARCHAR(255) NULL,
    ADD UNIQUE KEY uq_job_logs_external_event_id (job_run_id, external_event_id);
