CREATE TABLE IF NOT EXISTS worker_definition_history (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    worker_definition_id BIGINT NOT NULL,
    version INT UNSIGNED NOT NULL,
    payload JSON NOT NULL,
    changed_by VARCHAR(255) NOT NULL,
    changed_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    FOREIGN KEY (worker_definition_id) REFERENCES worker_definitions(id) ON DELETE CASCADE,
    UNIQUE KEY uk_worker_definition_version (worker_definition_id, version)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT INTO worker_definition_history (worker_definition_id, version, payload, changed_by, changed_at)
SELECT
    wd.id,
    1,
    JSON_OBJECT(
        'ワーカー名', wd.name,
        '説明', COALESCE(wd.description, ''),
        'ワーカータイプ', wd.worker_type,
        '設定', wd.config
    ),
    'system',
    COALESCE(wd.updated_at, wd.created_at, CURRENT_TIMESTAMP(3))
FROM worker_definitions wd
WHERE NOT EXISTS (
    SELECT 1
    FROM worker_definition_history h
    WHERE h.worker_definition_id = wd.id
);

ALTER TABLE worker_definitions DROP COLUMN is_active;
