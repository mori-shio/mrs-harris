-- 1. test-job を挿入 (存在しない場合)
INSERT INTO jobs (id, name, description, job_type, payload, schedule_expr, worker_type, retry_policy, timeout_sec, is_active, tags, worker_definition_id, space_id)
VALUES (
    '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6',
    'test-job',
    'This is a test job',
    'one_shot',
    '{"args": ["hello", "world"], "command": "echo"}',
    NULL,
    'fargate',
    '{"backoff": "exponential", "max_retries": 3, "base_delay_sec": 10}',
    3600,
    1,
    '["test", "local"]',
    'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d',
    NULL
)
ON DUPLICATE KEY UPDATE name=name;

-- 2. test-job の既存の実行履歴を削除 (クリーンアップして常に12件にする)
DELETE FROM job_runs WHERE job_id = '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6';

-- 3. test-job の実行履歴を12件挿入 (作成時間を変えて順序が正しくなるようにする)
INSERT INTO job_runs (id, job_id, status, worker_type, trigger_type, attempt, duration_ms, created_at, started_at, finished_at, worker_definition_id)
VALUES
('00000000-0000-0000-0000-000000000001', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'failed', 'fargate', 'manual', 1, 1500, '2026-05-25 00:00:01', '2026-05-25 00:00:01', '2026-05-25 00:00:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000002', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 2000, '2026-05-25 00:05:00', '2026-05-25 00:05:00', '2026-05-25 00:05:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000003', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 1800, '2026-05-25 00:10:00', '2026-05-25 00:10:00', '2026-05-25 00:10:01', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000004', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'failed', 'fargate', 'manual', 1, 3000, '2026-05-25 00:15:00', '2026-05-25 00:15:00', '2026-05-25 00:15:03', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000005', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 2100, '2026-05-25 00:20:00', '2026-05-25 00:20:00', '2026-05-25 00:20:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000006', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 1900, '2026-05-25 00:25:00', '2026-05-25 00:25:00', '2026-05-25 00:25:01', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000007', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'failed', 'fargate', 'manual', 1, 4000, '2026-05-25 00:30:00', '2026-05-25 00:30:00', '2026-05-25 00:30:04', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000008', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 2300, '2026-05-25 00:35:00', '2026-05-25 00:35:00', '2026-05-25 00:35:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000009', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 1700, '2026-05-25 00:40:00', '2026-05-25 00:40:00', '2026-05-25 00:40:01', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000010', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'failed', 'fargate', 'manual', 1, 2200, '2026-05-25 00:45:00', '2026-05-25 00:45:00', '2026-05-25 00:45:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000011', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 2500, '2026-05-25 00:50:00', '2026-05-25 00:50:00', '2026-05-25 00:50:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'),
('00000000-0000-0000-0000-000000000012', '1f6cd101-7fff-44f1-92b1-9a8e9148ffe6', 'succeeded', 'fargate', 'manual', 1, 2000, '2026-05-25 00:55:00', '2026-05-25 00:55:00', '2026-05-25 00:55:02', 'd1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d');
