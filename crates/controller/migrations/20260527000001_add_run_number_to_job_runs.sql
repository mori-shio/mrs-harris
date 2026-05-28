-- 1. NULL許容で run_number カラムを追加
ALTER TABLE job_runs ADD COLUMN run_number INT UNSIGNED NULL;

-- 2. 既存レコードに対して、各 job_id ごとに created_at, id 順で連番を計算してバックフィル
-- MySQL 8.0 の ROW_NUMBER() を使用した UPDATE JOIN 構文
UPDATE job_runs r
JOIN (
    SELECT id, ROW_NUMBER() OVER (PARTITION BY job_id ORDER BY created_at ASC, id ASC) as seq
    FROM job_runs
) seq_data ON r.id = seq_data.id
SET r.run_number = seq_data.seq;

-- 3. run_number を NOT NULL に変更
ALTER TABLE job_runs MODIFY COLUMN run_number INT UNSIGNED NOT NULL;

-- 4. (job_id, run_number) に対するユニークキー制約を追加
ALTER TABLE job_runs ADD UNIQUE KEY uq_job_run_number (job_id, run_number);
