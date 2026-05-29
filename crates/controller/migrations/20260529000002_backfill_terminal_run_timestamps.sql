UPDATE job_runs
SET
    finished_at = COALESCE(finished_at, updated_at),
    duration_ms = COALESCE(
        duration_ms,
        CASE
            WHEN started_at IS NOT NULL THEN GREATEST(TIMESTAMPDIFF(MICROSECOND, started_at, COALESCE(finished_at, updated_at)), 0) DIV 1000
            ELSE NULL
        END
    )
WHERE status IN ('succeeded', 'failed', 'cancelled', 'dead_letter')
  AND (finished_at IS NULL OR duration_ms IS NULL);
