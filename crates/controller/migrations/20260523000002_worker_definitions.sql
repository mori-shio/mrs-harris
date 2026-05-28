-- Mrs. Harris 自作ワーカー定義スキーマの追加と既存データの移行

-- 1. 自作ワーカー定義テーブルの追加
CREATE TABLE IF NOT EXISTS worker_definitions (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    name          VARCHAR(255) NOT NULL UNIQUE,
    description   TEXT,
    worker_type   ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate',
    config        JSON NOT NULL,
    is_active     TINYINT(1) NOT NULL DEFAULT 1,
    created_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 2. デフォルトのワーカー定義レコードを挿入 (Fargate & Lambda)
INSERT INTO worker_definitions (id, name, description, worker_type, config, is_active)
VALUES 
(1, 'default-fargate', 'デフォルトのAWS Fargate実行ワーカー', 'fargate', '{}', 1),
(2, 'default-lambda', 'デフォルトのAWS Lambda実行ワーカー', 'lambda', '{}', 1)
ON DUPLICATE KEY UPDATE name=name;

-- 3. jobs テーブルに worker_definition_id を追加して外部キー設定
ALTER TABLE jobs ADD COLUMN worker_definition_id BIGINT NULL;
ALTER TABLE jobs ADD CONSTRAINT fk_jobs_worker_definition FOREIGN KEY (worker_definition_id) REFERENCES worker_definitions(id) ON DELETE SET NULL;

-- 既存の jobs レコードを対応するデフォルトの定義に移行
UPDATE jobs SET worker_definition_id = 1 WHERE worker_type = 'fargate' AND worker_definition_id IS NULL;
UPDATE jobs SET worker_definition_id = 2 WHERE worker_type = 'lambda' AND worker_definition_id IS NULL;

-- 4. job_runs テーブルに worker_definition_id を追加
ALTER TABLE job_runs ADD COLUMN worker_definition_id BIGINT NULL;

-- 既存の job_runs レコードを移行
UPDATE job_runs SET worker_definition_id = 1 WHERE worker_type = 'fargate' AND worker_definition_id IS NULL;
UPDATE job_runs SET worker_definition_id = 2 WHERE worker_type = 'lambda' AND worker_definition_id IS NULL;
