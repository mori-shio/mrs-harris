ALTER TABLE job_runs
    ADD COLUMN worker_definition_history_id BIGINT NULL AFTER job_history_id;

UPDATE job_runs r
JOIN jobs j ON j.id = r.job_id
JOIN worker_definition_history wdh
  ON wdh.worker_definition_id = j.worker_definition_id
LEFT JOIN worker_definition_history wdh_newer
  ON wdh_newer.worker_definition_id = wdh.worker_definition_id
 AND wdh_newer.version > wdh.version
SET r.worker_definition_history_id = wdh.id
WHERE r.worker_definition_history_id IS NULL
  AND wdh_newer.id IS NULL;

ALTER TABLE job_runs
    MODIFY COLUMN worker_definition_history_id BIGINT NOT NULL;

ALTER TABLE job_runs
    ADD CONSTRAINT fk_job_runs_worker_definition_history
        FOREIGN KEY (worker_definition_history_id) REFERENCES worker_definition_history(id);

ALTER TABLE workers
    ADD COLUMN worker_definition_history_id BIGINT NULL AFTER id;

UPDATE workers w
JOIN job_runs r ON r.id = w.job_run_id
SET w.worker_definition_history_id = r.worker_definition_history_id
WHERE w.worker_definition_history_id IS NULL;

ALTER TABLE workers
    MODIFY COLUMN worker_definition_history_id BIGINT NOT NULL;

ALTER TABLE workers
    ADD CONSTRAINT fk_workers_definition_history
        FOREIGN KEY (worker_definition_history_id) REFERENCES worker_definition_history(id);

ALTER TABLE workers
    DROP FOREIGN KEY fk_workers_definition;

ALTER TABLE workers
    DROP COLUMN worker_definition_id;
