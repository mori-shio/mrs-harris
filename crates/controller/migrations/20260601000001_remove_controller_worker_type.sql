-- Normalize legacy controller worker definitions to local lambda fallback
-- and remove the controller worker_type enum value.

UPDATE worker_definitions
SET
    name = CASE
        WHEN name = 'default-controller' THEN 'default-local-lambda'
        ELSE name
    END,
    description = CASE
        WHEN name = 'default-controller' OR description = 'Harris Controller本体でバックグラウンド実行するデフォルトワーカー'
            THEN 'ローカル fallback 実行用のデフォルト Lambda ワーカー'
        ELSE description
    END,
    worker_type = 'lambda',
    config = JSON_SET(
        CASE
            WHEN config IS NULL OR JSON_TYPE(config) = 'NULL' THEN JSON_OBJECT()
            ELSE config
        END,
        '$.function_name',
        'local'
    )
WHERE worker_type = 'controller';

UPDATE workers
SET worker_type = 'lambda'
WHERE worker_type = 'controller';

UPDATE dag_tasks
SET worker_type = 'lambda'
WHERE worker_type = 'controller';

UPDATE jobs
SET description = 'Lambda local fallback で実行される軽量なテストジョブ'
WHERE name = 'local-echo-job' AND worker_definition_id = 1001;

UPDATE job_history
SET payload = '{"タグ": ["local", "test"], "説明": "Lambda local fallback で実行される軽量なテストジョブ", "ジョブ名": "local-echo-job", "ジョブタイプ": "OneShot (単発/手動実行)", "タイムアウト": "300 秒", "有効化状態": "有効", "初期遅延": "5 秒", "リトライ上限": "1", "バックオフ戦略": "固定", "ワーカー定義": "default-local-lambda", "スクリプト": "echo Hello from Controller!", "Slack通知": {"成功時": "無効", "失敗時": "無効", "ジョブ起動時": "無効"}}'
WHERE job_id = 1001 AND version = 1;

ALTER TABLE worker_definitions
MODIFY COLUMN worker_type ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate';

ALTER TABLE workers
MODIFY COLUMN worker_type ENUM('fargate', 'lambda') NOT NULL;

ALTER TABLE dag_tasks
MODIFY COLUMN worker_type ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate';
