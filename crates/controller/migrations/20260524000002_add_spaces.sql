-- スペースマスタテーブルの作成
CREATE TABLE IF NOT EXISTS spaces (
    id          CHAR(36) PRIMARY KEY,
    name        VARCHAR(255) UNIQUE NOT NULL,
    description TEXT,
    created_at  TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3),
    updated_at  TIMESTAMP(3) NOT NULL DEFAULT CURRENT_TIMESTAMP(3) ON UPDATE CURRENT_TIMESTAMP(3)
) ENGINE=InnoDB DEFAULT CHARSET=utf8mb4;

-- ジョブテーブルにスペース外部キーを追加
ALTER TABLE jobs ADD COLUMN space_id CHAR(36) NULL;
ALTER TABLE jobs ADD CONSTRAINT fk_jobs_space FOREIGN KEY (space_id) REFERENCES spaces(id) ON DELETE SET NULL;

-- 初期テストデータ（hoge & huga スペース）の自動挿入
INSERT INTO spaces (id, name, description) VALUES 
('s1111111-1111-1111-1111-111111111111', 'hoge', 'hogeスペースのジョブ管理領域'),
('s2222222-2222-2222-2222-222222222222', 'huga', 'hugaスペースのジョブ管理領域');
