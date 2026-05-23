-- ジョブ設定変更履歴
CREATE TABLE IF NOT EXISTS job_history (
    id          CHAR(36) PRIMARY KEY,
    job_id      CHAR(36) NOT NULL,
    version     INT UNSIGNED NOT NULL,
    payload     JSON NOT NULL,
    changed_by  VARCHAR(255) NOT NULL,
    changed_at  TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    UNIQUE KEY uk_job_version (job_id, version)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
