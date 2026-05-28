-- Controllerワーカータイプのテストデータの追加

-- 1. Controllerワーカー定義の挿入
INSERT INTO worker_definitions (id, name, description, worker_type, config, is_active)
VALUES (
    '1001',
    'default-controller',
    'Harris Controller本体でバックグラウンド実行するデフォルトワーカー',
    'controller',
    '{}',
    1
);

-- 2. Controllerワーカー定義を使用するジョブの挿入
INSERT INTO jobs (id, name, description, job_type, payload, schedule_expr, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id)
VALUES
(
    '1001',
    'local-echo-job',
    'Controller本体で実行される軽量なテストジョブ',
    'one_shot',
    '{"command": "echo", "args": ["Hello", "from", "Controller!"]}',
    NULL,
    '{"backoff": "fixed", "max_retries": 1, "base_delay_sec": 5}',
    300,
    1,
    '["local", "test"]',
    '1001', -- default-controller
    '1'    -- hoge space
);

-- 3. 設定変更履歴 (job_history) のインサート
INSERT INTO job_history (job_id, version, payload, changed_by, changed_at)
VALUES
(
    '1001',
    1,
    '{"タグ": ["local", "test"], "説明": "Controller本体で実行される軽量なテストジョブ", "ジョブ名": "local-echo-job", "ジョブタイプ": "OneShot (単発/手動実行)", "タイムアウト": "300 秒", "有効化状態": "有効", "初期遅延": "5 秒", "リトライ上限": "1", "バックオフ戦略": "固定", "ワーカー定義": "default-controller", "スクリプト / DAG構成": "echo Hello from Controller!", "Slack通知": {"成功時": "無効", "失敗時": "無効", "ジョブ起動時": "無効"}}',
    'system-admin',
    '2026-05-26 12:00:00.000'
);

-- 4. ジョブ実行履歴 (job_runs) の挿入
INSERT INTO job_runs (id, job_id, status, trigger_type, attempt, duration_ms, created_at, started_at, finished_at)
VALUES
('1001', '1001', 'succeeded', 'manual', 1, 100, '2026-05-26 12:05:00', '2026-05-26 12:05:00', '2026-05-26 12:05:01');
