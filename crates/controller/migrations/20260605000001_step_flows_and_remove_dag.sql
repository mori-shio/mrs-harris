-- StepFlow replaces DAG as a standalone orchestration concept.

DELETE FROM task_runs
WHERE run_id IN (
    SELECT id FROM job_runs WHERE job_id IN (SELECT id FROM jobs WHERE job_type = 'dag')
);

DELETE FROM job_runs WHERE job_id IN (SELECT id FROM jobs WHERE job_type = 'dag');
DELETE FROM job_history WHERE job_id IN (SELECT id FROM jobs WHERE job_type = 'dag');
DELETE FROM dag_edges WHERE dag_id IN (SELECT id FROM jobs WHERE job_type = 'dag');
DELETE FROM dag_tasks WHERE dag_id IN (SELECT id FROM jobs WHERE job_type = 'dag');
DELETE FROM jobs WHERE job_type = 'dag';

ALTER TABLE jobs MODIFY COLUMN job_type ENUM('cron', 'one_shot') NOT NULL;
ALTER TABLE job_runs MODIFY COLUMN trigger_type ENUM('scheduled', 'manual', 'dependency', 'step_flow') NOT NULL;

CREATE TABLE IF NOT EXISTS step_flows (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    name VARCHAR(255) UNIQUE NOT NULL,
    description TEXT,
    space_id BIGINT NULL,
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    timeout_sec INT UNSIGNED NOT NULL DEFAULT 3600,
    tags JSON NOT NULL,
    created_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    CONSTRAINT fk_step_flows_space FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS step_flow_groups (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    step_flow_id BIGINT NOT NULL,
    group_order INT UNSIGNED NOT NULL,
    run_condition ENUM('on_success', 'always') NULL,
    created_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    UNIQUE KEY uk_step_flow_group_order (step_flow_id, group_order),
    CONSTRAINT fk_step_flow_groups_flow FOREIGN KEY (step_flow_id) REFERENCES step_flows(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS step_flow_steps (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    group_id BIGINT NOT NULL,
    step_order INT UNSIGNED NOT NULL,
    job_id BIGINT NOT NULL,
    created_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    UNIQUE KEY uk_step_flow_step_order (group_id, step_order),
    KEY idx_step_flow_steps_job (job_id),
    CONSTRAINT fk_step_flow_steps_group FOREIGN KEY (group_id) REFERENCES step_flow_groups(id) ON DELETE CASCADE,
    CONSTRAINT fk_step_flow_steps_job FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE RESTRICT
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS step_flow_history (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    step_flow_id BIGINT NOT NULL,
    version INT UNSIGNED NOT NULL,
    payload JSON NOT NULL,
    changed_by VARCHAR(255) NOT NULL,
    changed_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    UNIQUE KEY uk_step_flow_history_version (step_flow_id, version),
    CONSTRAINT fk_step_flow_history_flow FOREIGN KEY (step_flow_id) REFERENCES step_flows(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS step_flow_runs (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    step_flow_id BIGINT NOT NULL,
    step_flow_history_id BIGINT NOT NULL,
    run_number BIGINT NOT NULL,
    status ENUM('pending', 'scheduled', 'queued', 'running', 'succeeded', 'failed', 'retrying', 'cancelled', 'dead_letter') NOT NULL,
    trigger_type ENUM('scheduled', 'manual', 'dependency', 'step_flow') NOT NULL,
    created_by VARCHAR(255) NOT NULL,
    started_at TIMESTAMP(3) NULL,
    finished_at TIMESTAMP(3) NULL,
    duration_ms BIGINT NULL,
    created_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    UNIQUE KEY uk_step_flow_run_number (step_flow_id, run_number),
    KEY idx_step_flow_runs_status (status),
    CONSTRAINT fk_step_flow_runs_flow FOREIGN KEY (step_flow_id) REFERENCES step_flows(id) ON DELETE CASCADE,
    CONSTRAINT fk_step_flow_runs_history FOREIGN KEY (step_flow_history_id) REFERENCES step_flow_history(id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

CREATE TABLE IF NOT EXISTS step_flow_step_runs (
    id BIGINT AUTO_INCREMENT PRIMARY KEY,
    step_flow_run_id BIGINT NOT NULL,
    step_flow_step_id BIGINT NOT NULL,
    job_id BIGINT NOT NULL,
    job_history_id BIGINT NOT NULL,
    job_run_id BIGINT NULL,
    status ENUM('pending', 'scheduled', 'queued', 'running', 'succeeded', 'failed', 'retrying', 'cancelled', 'dead_letter') NOT NULL,
    started_at TIMESTAMP(3) NULL,
    finished_at TIMESTAMP(3) NULL,
    created_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    UNIQUE KEY uk_step_flow_step_run_once (step_flow_run_id, step_flow_step_id),
    CONSTRAINT fk_step_flow_step_runs_flow_run FOREIGN KEY (step_flow_run_id) REFERENCES step_flow_runs(id) ON DELETE CASCADE,
    CONSTRAINT fk_step_flow_step_runs_step FOREIGN KEY (step_flow_step_id) REFERENCES step_flow_steps(id),
    CONSTRAINT fk_step_flow_step_runs_job FOREIGN KEY (job_id) REFERENCES jobs(id),
    CONSTRAINT fk_step_flow_step_runs_history FOREIGN KEY (job_history_id) REFERENCES job_history(id),
    CONSTRAINT fk_step_flow_step_runs_job_run FOREIGN KEY (job_run_id) REFERENCES job_runs(id) ON DELETE SET NULL
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

INSERT INTO step_flows (id, name, description, space_id, is_active, timeout_sec, tags, created_at, updated_at)
VALUES (
    1001,
    'step-flow',
    '複数の登録済みジョブをグループ単位で直列・並列実行するサンプルステップフロー',
    1,
    1,
    3600,
    '["step-flow", "sample"]',
    '2026-06-05 00:00:00.000',
    '2026-06-05 00:00:00.000'
);

INSERT INTO step_flow_groups (id, step_flow_id, group_order, run_condition)
VALUES
(1001, 1001, 1, NULL),
(1002, 1001, 2, 'on_success'),
(1003, 1001, 3, 'always');

INSERT INTO step_flow_steps (id, group_id, step_order, job_id)
VALUES
(1001, 1001, 1, 101),
(1002, 1001, 2, 102),
(1003, 1002, 1, 1001),
(1004, 1003, 1, 1001);

INSERT INTO step_flow_history (id, step_flow_id, version, payload, changed_by, changed_at)
VALUES (
    1001,
    1001,
    1,
    JSON_OBJECT(
        'ステップフロー名', 'step-flow',
        '説明', '複数の登録済みジョブをグループ単位で直列・並列実行するサンプルステップフロー',
        'タグ', JSON_ARRAY('step-flow', 'sample'),
        '有効化状態', '有効',
        'グループ', JSON_ARRAY(
            JSON_OBJECT('順序', 1, '実行条件', NULL, 'ジョブ', JSON_ARRAY('test-job', 'cron-fargate-job')),
            JSON_OBJECT('順序', 2, '実行条件', '前Group成功時のみ', 'ジョブ', JSON_ARRAY('local-echo-job')),
            JSON_OBJECT('順序', 3, '実行条件', '常に実行', 'ジョブ', JSON_ARRAY('local-echo-job'))
        )
    ),
    'system-admin',
    '2026-06-05 00:00:00.000'
);

DROP TABLE IF EXISTS task_runs;
DROP TABLE IF EXISTS dag_edges;
DROP TABLE IF EXISTS dag_tasks;
