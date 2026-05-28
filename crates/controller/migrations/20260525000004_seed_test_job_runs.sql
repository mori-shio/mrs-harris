-- バリエーション豊かなジョブ定義・実行履歴・設定履歴の初期シードデータ再構築

-- 1. 既存テストデータのクリーンアップ (依存関係の順序を考慮)
DELETE FROM task_runs;
DELETE FROM job_logs;
DELETE FROM dag_edges;
DELETE FROM dag_tasks;
DELETE FROM job_history;
DELETE FROM job_runs;
DELETE FROM jobs;
DELETE FROM worker_definitions WHERE id = '100';

-- 2. カスタム独自ワーカー定義の挿入
INSERT INTO worker_definitions (id, name, description, worker_type, config, is_active)
VALUES (
    '100',
    'custom-fargate-large',
    '機械学習やデータ分析用の大容量カスタムECS Fargateワーカー',
    'fargate',
    '{"cpu": "2048", "memory": "4096", "env": {"CUDA_VISIBLE_DEVICES": "0"}}',
    1
);

-- 3. 多様なジョブ定義の挿入 (OneShot, Cron, DAG / Fargate, Lambda, Custom)
INSERT INTO jobs (id, name, description, job_type, payload, schedule_expr, worker_type, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id)
VALUES
-- (1) test-job (OneShot / Fargate)
(
    '101',
    'test-job',
    '設定バージョン履歴 (v1/v2) の比較検証用OneShotジョブ',
    'one_shot',
    '{"args": ["hello", "world"], "command": "echo"}',
    NULL,
    'fargate',
    '{"backoff": "exponential", "max_retries": 3, "base_delay_sec": 10}',
    3600,
    1,
    '["test", "local"]',
    '1', -- default-fargate
    '1'  -- hogeスペース
),
-- (2) cron-fargate-job (Cron定期実行 / Fargate)
(
    '102',
    'cron-fargate-job',
    '5分毎に起動して定期バックアップを行う自動化ジョブ',
    'cron',
    '{"command": "echo \\"Running scheduled DB backup...\\""}',
    '0 */5 * * * *',
    'fargate',
    '{"backoff": "fixed", "max_retries": 2, "base_delay_sec": 30}',
    1800,
    1,
    '["backup", "cron"]',
    '1', -- default-fargate
    '1'  -- hogeスペース
),
-- (3) dag-lambda-job (DAGパイプライン / Lambda)
(
    '103',
    'dag-lambda-job',
    'Lambda上で3つの依存タスクを直列実行するデータ収集DAG',
    'dag',
    '{}',
    NULL,
    'lambda',
    '{"backoff": "linear", "max_retries": 1, "base_delay_sec": 5}',
    600,
    1,
    '["pipeline", "lambda"]',
    '2', -- default-lambda
    '2'  -- hugaスペース
),
-- (4) custom-node-job (OneShot / カスタム独自ワーカー定義)
(
    '104',
    'custom-node-job',
    '大容量大プロセッサカスタム定義(custom-fargate-large)で実行するタスク',
    'one_shot',
    '{"command": "python train.py --epochs 10"}',
    NULL,
    'fargate',
    '{"backoff": "exponential", "max_retries": 3, "base_delay_sec": 10}',
    7200,
    1,
    '["ml", "heavy"]',
    '100', -- custom-fargate-large
    '2'  -- hugaスペース
),
-- (5) history-heavy-job (OneShot / Fargate)
(
    '105',
    'history-heavy-job',
    '設定変更履歴が13件存在するテスト用ジョブ',
    'one_shot',
    '{"command": "echo test"}',
    NULL,
    'fargate',
    '{"backoff": "fixed", "max_retries": 1, "base_delay_sec": 5}',
    600,
    1,
    '["history", "test"]',
    '1',
    '1'
);

-- 4. 設定変更履歴 (job_history) の事前インサート (v1 / v2 設定差分検証用)
INSERT INTO job_history (job_id, version, payload, changed_by, changed_at)
VALUES
-- test-job v1 (初期設定)
(
    '101',
    1,
    '{"タグ": ["test", "local"], "説明": "設定バージョン履歴 (v1/v2) の比較検証用OneShotジョブ", "ジョブ名": "test-job", "ジョブタイプ": "OneShot (単発/手動実行)", "タイムアウト": "3600 秒", "有効化状態": "有効", "初期遅延": "10 秒", "リトライ上限": "3", "バックオフ戦略": "指数", "ワーカー定義": "default-fargate", "スクリプト / DAG構成": "echo hello world", "Slack通知": {"成功時": "無効", "失敗時": "無効", "ジョブ起動時": "無効"}}',
    'system-admin',
    '2026-05-24 10:00:00.000'
),
-- test-job v2 (説明修正、引数追加、Slack失敗時通知の有効化)
(
    '101',
    2,
    '{"タグ": ["test", "local", "production"], "説明": "設定バージョン履歴 (v1/v2) の比較検証用OneShotジョブ (v2拡張)", "ジョブ名": "test-job", "ジョブタイプ": "OneShot (単発/手動実行)", "タイムアウト": "3600 秒", "有効化状態": "有効", "初期遅延": "10 秒", "リトライ上限": "3", "バックオフ戦略": "指数", "ワーカー定義": "default-fargate", "スクリプト / DAG構成": "echo hello world --verbose", "Slack通知": {"成功時": "無効", "失敗時": "有効", "ジョブ起動時": "無効"}}',
    'user-shion',
    '2026-05-25 12:00:00.000'
),
-- cron-fargate-job v1
(
    '102',
    1,
    '{"タグ": ["backup", "cron"], "説明": "5分毎に起動して定期バックアップを行う自動化ジョブ", "ジョブ名": "cron-fargate-job", "ジョブタイプ": "Cron (定期実行)", "スケジュール (Cron)": "0 */5 * * * *", "タイムアウト": "1800 秒", "有効化状態": "有効", "初期遅延": "30 秒", "リトライ上限": "2", "バックオフ戦略": "固定", "ワーカー定義": "default-fargate", "スクリプト / DAG構成": "echo \\"Running scheduled DB backup...\\"", "Slack通知": {"成功時": "有効", "失敗時": "有効", "ジョブ起動時": "無効"}}',
    'system-admin',
    '2026-05-24 10:05:00.000'
),
-- dag-lambda-job v1
(
    '103',
    1,
    '{"タグ": ["pipeline", "lambda"], "説明": "Lambda上で3つの依存タスクを直列実行するデータ収集DAG", "ジョブ名": "dag-lambda-job", "ジョブタイプ": "DAG (タスクパイプライン)", "タイムアウト": "600 秒", "有効化状態": "有効", "初期遅延": "5 秒", "リトライ上限": "1", "バックオフ戦略": "線形", "ワーカー定義": "default-lambda", "スクリプト / DAG構成": [{"name": "task-A", "payload": {"command": "echo A"}, "worker_type": "lambda"}, {"name": "task-B", "payload": {"command": "echo B"}, "worker_type": "lambda"}, {"name": "task-C", "payload": {"command": "echo C"}, "worker_type": "lambda"}], "Slack通知": {"成功時": "無効", "失敗時": "有効", "ジョブ起動時": "無効"}}',
    'system-admin',
    '2026-05-24 10:10:00.000'
),
-- custom-node-job v1
(
    '104',
    1,
    '{"タグ": ["ml", "heavy"], "説明": "大容量大プロセッサカスタム定義(custom-fargate-large)で実行するタスク", "ジョブ名": "custom-node-job", "ジョブタイプ": "OneShot (単発/手動実行)", "タイムアウト": "7200 秒", "有効化状態": "有効", "初期遅延": "10 秒", "リトライ上限": "3", "バックオフ戦略": "指数", "ワーカー定義": "custom-fargate-large", "スクリプト / DAG構成": "python train.py --epochs 10", "Slack通知": {"成功時": "無効", "失敗時": "無効", "ジョブ起動時": "無効"}}',
    'user-shion',
    '2026-05-24 10:15:00.000'
),
-- history-heavy-job v1 to v13
(
    '105',
    1,
    '{"説明": "v1 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:01:00.000'
),
(
    '105',
    2,
    '{"説明": "v2 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:02:00.000'
),
(
    '105',
    3,
    '{"説明": "v3 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:03:00.000'
),
(
    '105',
    4,
    '{"説明": "v4 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:04:00.000'
),
(
    '105',
    5,
    '{"説明": "v5 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:05:00.000'
),
(
    '105',
    6,
    '{"説明": "v6 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:06:00.000'
),
(
    '105',
    7,
    '{"説明": "v7 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:07:00.000'
),
(
    '105',
    8,
    '{"説明": "v8 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:08:00.000'
),
(
    '105',
    9,
    '{"説明": "v9 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:09:00.000'
),
(
    '105',
    10,
    '{"説明": "v10 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:10:00.000'
),
(
    '105',
    11,
    '{"説明": "v11 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:11:00.000'
),
(
    '105',
    12,
    '{"説明": "v12 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:12:00.000'
),
(
    '105',
    13,
    '{"説明": "v13 の変更内容", "ジョブ名": "history-heavy-job"}',
    'system-admin',
    '2026-05-24 10:13:00.000'
);

-- 5. DAG構成タスク & 依存エッジの挿入
INSERT INTO dag_tasks (dag_id, task_name, payload, worker_type, retry_policy, timeout_sec)
VALUES
('103', 'task-A', '{"command": "echo \\"Running DAG task A...\\""}', 'lambda', NULL, NULL),
('103', 'task-B', '{"command": "echo \\"Running DAG task B...\\""}', 'lambda', NULL, NULL),
('103', 'task-C', '{"command": "echo \\"Running DAG task C...\\""}', 'lambda', NULL, NULL);

INSERT INTO dag_edges (dag_id, from_task, to_task)
VALUES
('103', 'task-A', 'task-B'),
('103', 'task-B', 'task-C');

-- 6. ジョブ実行履歴 (job_runs) の挿入 (config_version を指定し、整合性を完全に修復)
INSERT INTO job_runs (id, job_id, status, worker_type, trigger_type, attempt, duration_ms, created_at, started_at, finished_at, worker_definition_id, config_version)
VALUES
-- (1) test-job 実行履歴 12件 (前半6件は v1、後半6件は v2 を明示的に割り当て)
('108', '101', 'failed', 'fargate', 'manual', 1, 1500, '2026-05-25 00:00:01', '2026-05-25 00:00:01', '2026-05-25 00:00:02', '1', 1),
('109', '101', 'succeeded', 'fargate', 'manual', 1, 2000, '2026-05-25 00:05:00', '2026-05-25 00:05:00', '2026-05-25 00:05:02', '1', 1),
('110', '101', 'succeeded', 'fargate', 'manual', 1, 1800, '2026-05-25 00:10:00', '2026-05-25 00:10:00', '2026-05-25 00:10:01', '1', 1),
('111', '101', 'failed', 'fargate', 'manual', 1, 3000, '2026-05-25 00:15:00', '2026-05-25 00:15:00', '2026-05-25 00:15:03', '1', 1),
('112', '101', 'succeeded', 'fargate', 'manual', 1, 2100, '2026-05-25 00:20:00', '2026-05-25 00:20:00', '2026-05-25 00:20:02', '1', 1),
('113', '101', 'succeeded', 'fargate', 'manual', 1, 1900, '2026-05-25 00:25:00', '2026-05-25 00:25:00', '2026-05-25 00:25:01', '1', 1),
('114', '101', 'failed', 'fargate', 'manual', 1, 4000, '2026-05-25 00:30:00', '2026-05-25 00:30:00', '2026-05-25 00:30:04', '1', 2),
('115', '101', 'succeeded', 'fargate', 'manual', 1, 2300, '2026-05-25 00:35:00', '2026-05-25 00:35:00', '2026-05-25 00:35:02', '1', 2),
('116', '101', 'succeeded', 'fargate', 'manual', 1, 1700, '2026-05-25 00:40:00', '2026-05-25 00:40:00', '2026-05-25 00:40:01', '1', 2),
('117', '101', 'failed', 'fargate', 'manual', 1, 2200, '2026-05-25 00:45:00', '2026-05-25 00:45:00', '2026-05-25 00:45:02', '1', 2),
('118', '101', 'succeeded', 'fargate', 'manual', 1, 2500, '2026-05-25 00:50:00', '2026-05-25 00:50:00', '2026-05-25 00:50:02', '1', 2),
('119', '101', 'succeeded', 'fargate', 'manual', 1, 2000, '2026-05-25 00:55:00', '2026-05-25 00:55:00', '2026-05-25 00:55:02', '1', 2),

-- (2) cron-fargate-job 実行履歴 3件 (定期実行)
('120', '102', 'succeeded', 'fargate', 'scheduled', 1, 1500, '2026-05-25 01:00:00', '2026-05-25 01:00:00', '2026-05-25 01:00:01', '1', 1),
('121', '102', 'succeeded', 'fargate', 'scheduled', 1, 1400, '2026-05-25 01:05:00', '2026-05-25 01:05:00', '2026-05-25 01:05:01', '1', 1),
('122', '102', 'failed', 'fargate', 'scheduled', 1, 1200, '2026-05-25 01:10:00', '2026-05-25 01:10:00', '2026-05-25 01:10:01', '1', 1),

-- (3) dag-lambda-job 実行履歴 2件 (A ➔ B ➔ C フロー)
('123', '103', 'succeeded', 'lambda', 'manual', 1, 4500, '2026-05-25 01:15:00', '2026-05-25 01:15:00', '2026-05-25 01:15:04', '2', 1),
('124', '103', 'failed', 'lambda', 'manual', 1, 3200, '2026-05-25 01:20:00', '2026-05-25 01:20:00', '2026-05-25 01:20:03', '2', 1),

-- (4) custom-node-job 実行履歴 1件 (カスタムワーカー)
('125', '104', 'succeeded', 'fargate', 'manual', 1, 8500, '2026-05-25 01:30:00', '2026-05-25 01:30:00', '2026-05-25 01:30:08', '100', 1);

-- 7. DAG個別タスクの実行履歴 (task_runs) の挿入 (グラフ表示とタスク一覧用)
INSERT INTO task_runs (run_id, task_name, status, worker_id, attempt, started_at, finished_at, duration_ms, output, error)
VALUES
-- Run 201 (全タスク成功フロー)
('123', 'task-A', 'succeeded', 'lambda-req-001', 1, '2026-05-25 01:15:00', '2026-05-25 01:15:01', 1500, '{"result": "success A"}', NULL),
('123', 'task-B', 'succeeded', 'lambda-req-002', 1, '2026-05-25 01:15:01', '2026-05-25 01:15:03', 1800, '{"result": "success B"}', NULL),
('123', 'task-C', 'succeeded', 'lambda-req-003', 1, '2026-05-25 01:15:03', '2026-05-25 01:15:04', 1200, '{"result": "success C"}', NULL),

-- Run 202 (タスクB失敗 ➔ タスクCスキップフロー)
('124', 'task-A', 'succeeded', 'lambda-req-004', 1, '2026-05-25 01:20:00', '2026-05-25 01:20:01', 1400, '{"result": "success A"}', NULL),
('124', 'task-B', 'failed', 'lambda-req-005', 1, '2026-05-25 01:20:01', '2026-05-25 01:20:03', 1800, NULL, 'Runtime error: memory limit exceeded'),
('124', 'task-C', 'skipped', NULL, 1, NULL, NULL, NULL, NULL, 'Skipped due to upstream task-B failure');
