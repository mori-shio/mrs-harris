-- Mrs. Harris 初期スキーマ

-- ユーザー管理
CREATE TABLE IF NOT EXISTS users (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    username      VARCHAR(255) NOT NULL UNIQUE,
    password_hash TEXT NOT NULL,
    role          VARCHAR(50) NOT NULL DEFAULT 'admin',
    created_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ジョブ定義
CREATE TABLE IF NOT EXISTS jobs (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    name          VARCHAR(255) NOT NULL UNIQUE,
    description   TEXT,
    job_type      ENUM('cron', 'dag', 'one_shot') NOT NULL,
    payload       JSON NOT NULL,
    schedule_expr VARCHAR(255),
    worker_type   ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate',
    retry_policy  JSON NOT NULL,
    timeout_sec   INT UNSIGNED NOT NULL DEFAULT 3600,
    is_active     TINYINT(1) NOT NULL DEFAULT 1,
    tags          JSON NOT NULL DEFAULT ('[]'),
    created_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    INDEX idx_jobs_active (is_active),
    INDEX idx_jobs_type (job_type)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- DAG タスク定義
CREATE TABLE IF NOT EXISTS dag_tasks (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    dag_id        BIGINT NOT NULL,
    task_name     VARCHAR(255) NOT NULL,
    payload       JSON NOT NULL,
    worker_type   ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate',
    retry_policy  JSON,
    timeout_sec   INT UNSIGNED,
    UNIQUE KEY uk_dag_task (dag_id, task_name),
    FOREIGN KEY (dag_id) REFERENCES jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- DAG エッジ定義
CREATE TABLE IF NOT EXISTS dag_edges (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    dag_id        BIGINT NOT NULL,
    from_task     VARCHAR(255) NOT NULL,
    to_task       VARCHAR(255) NOT NULL,
    UNIQUE KEY uk_dag_edge (dag_id, from_task, to_task),
    FOREIGN KEY (dag_id) REFERENCES jobs(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ジョブ実行履歴
CREATE TABLE IF NOT EXISTS job_runs (
    id              BIGINT AUTO_INCREMENT PRIMARY KEY,
    job_id          BIGINT NOT NULL,
    status          ENUM('pending','scheduled','queued','running',
                         'succeeded','failed','retrying','cancelled',
                         'dead_letter') NOT NULL DEFAULT 'pending',
    worker_type     ENUM('fargate', 'lambda') NOT NULL DEFAULT 'fargate',
    worker_id       VARCHAR(512),
    trigger_type    ENUM('scheduled','manual','dependency') NOT NULL,
    attempt         INT UNSIGNED NOT NULL DEFAULT 1,
    scheduled_at    TIMESTAMP(3) NULL,
    started_at      TIMESTAMP(3) NULL,
    finished_at     TIMESTAMP(3) NULL,
    next_retry_at   TIMESTAMP(3) NULL,
    duration_ms     BIGINT,
    output          JSON,
    error           TEXT,
    version         INT UNSIGNED NOT NULL DEFAULT 1,
    created_at      TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at      TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3),
    FOREIGN KEY (job_id) REFERENCES jobs(id),
    INDEX idx_runs_status (status),
    INDEX idx_runs_job_created (job_id, created_at DESC),
    INDEX idx_runs_scheduled (scheduled_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- DAG タスク実行履歴
CREATE TABLE IF NOT EXISTS task_runs (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    run_id        BIGINT NOT NULL,
    task_name     VARCHAR(255) NOT NULL,
    status        ENUM('pending','queued','running','succeeded',
                       'failed','retrying','skipped') NOT NULL DEFAULT 'pending',
    worker_id     VARCHAR(512),
    attempt       INT UNSIGNED NOT NULL DEFAULT 1,
    started_at    TIMESTAMP(3) NULL,
    finished_at   TIMESTAMP(3) NULL,
    duration_ms   BIGINT,
    output        JSON,
    error         TEXT,
    created_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    FOREIGN KEY (run_id) REFERENCES job_runs(id) ON DELETE CASCADE,
    INDEX idx_task_runs_run (run_id),
    INDEX idx_task_runs_status (status)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 実行ログ
CREATE TABLE IF NOT EXISTS job_logs (
    id          BIGINT UNSIGNED AUTO_INCREMENT PRIMARY KEY,
    run_id      BIGINT NOT NULL,
    task_name   VARCHAR(255),
    stream      ENUM('stdout', 'stderr', 'system') NOT NULL,
    line        TEXT NOT NULL,
    logged_at   TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    FOREIGN KEY (run_id) REFERENCES job_runs(id) ON DELETE CASCADE,
    INDEX idx_logs_run (run_id, logged_at)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- 通知チャネル
CREATE TABLE IF NOT EXISTS notification_channels (
    id            BIGINT AUTO_INCREMENT PRIMARY KEY,
    name          VARCHAR(255) NOT NULL UNIQUE,
    channel_type  ENUM('slack', 'email') NOT NULL,
    config        JSON NOT NULL,
    is_active     TINYINT(1) NOT NULL DEFAULT 1,
    created_at    TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ジョブ通知紐付け
CREATE TABLE IF NOT EXISTS job_notifications (
    job_id      BIGINT NOT NULL,
    channel_id  BIGINT NOT NULL,
    on_events   JSON NOT NULL DEFAULT ('["failed","dead_letter"]'),
    PRIMARY KEY (job_id, channel_id),
    FOREIGN KEY (job_id) REFERENCES jobs(id) ON DELETE CASCADE,
    FOREIGN KEY (channel_id) REFERENCES notification_channels(id) ON DELETE CASCADE
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ワーカー実行トラッキング
CREATE TABLE IF NOT EXISTS worker_tracking (
    id              BIGINT AUTO_INCREMENT PRIMARY KEY,
    worker_type     ENUM('fargate', 'lambda') NOT NULL,
    external_id     VARCHAR(512) NOT NULL,
    status          ENUM('running', 'completed', 'failed', 'timed_out') NOT NULL DEFAULT 'running',
    run_id          BIGINT NOT NULL,
    started_at      TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    last_heartbeat  TIMESTAMP(3),
    metadata        JSON DEFAULT ('{}'),
    FOREIGN KEY (run_id) REFERENCES job_runs(id),
    INDEX idx_worker_tracking_status (status),
    INDEX idx_worker_tracking_run (run_id)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;
