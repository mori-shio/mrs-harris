-- Normalize local-echo-job history snapshots so v1/v2 comparisons only reflect
-- real config changes rather than snapshot schema drift between seed data and
-- current build_job_snapshot output.

UPDATE jobs
SET
    description = 'Lambda local fallback で実行される軽量なテストジョブ',
    payload = '{"command":"sh","args":["-c","set -eux\\n\\necho Hello from Controller!\\nsleep 10\\necho ''done.''"],"working_dir":null,"env":{},"ssm_region":"","ssm_path":"","ssm_recursive":false}'
WHERE id = 1001;

UPDATE job_history
SET payload = '{
  "説明": "Lambda local fallback で実行される軽量なテストジョブ",
  "ジョブ名": "local-echo-job",
  "ジョブタイプ": "OneShot (単発)",
  "スケジュール (Cron)": "未設定",
  "有効化状態": "有効",
  "ワーカー定義": "default-local-lambda",
  "タイムアウト": "300 秒",
  "リトライ上限": 1,
  "バックオフ戦略": "固定時間",
  "初期遅延": "5 秒",
  "タグ": ["local", "test"],
  "直接設定の環境変数": {},
  "SSMパラメータ連携": {
    "リージョン": "",
    "パス": "",
    "再帰取得": "無効"
  },
  "Slack通知": {
    "ジョブ起動時": "無効",
    "成功時": "無効",
    "失敗時": "無効"
  },
  "スクリプト": "echo Hello from Controller!"
}'
WHERE job_id = 1001 AND version = 1;

INSERT INTO job_history (job_id, version, payload, changed_by, changed_at)
SELECT
    1001,
    2,
    '{
      "説明": "Lambda local fallback で実行される軽量なテストジョブ",
      "ジョブ名": "local-echo-job",
      "ジョブタイプ": "OneShot (単発)",
      "スケジュール (Cron)": "未設定",
      "有効化状態": "有効",
      "ワーカー定義": "default-local-lambda",
      "タイムアウト": "300 秒",
      "リトライ上限": 1,
      "バックオフ戦略": "固定時間",
      "初期遅延": "5 秒",
      "タグ": ["local", "test"],
      "直接設定の環境変数": {},
      "SSMパラメータ連携": {
        "リージョン": "",
        "パス": "",
        "再帰取得": "無効"
      },
      "Slack通知": {
        "ジョブ起動時": "無効",
        "成功時": "無効",
        "失敗時": "無効"
      },
      "スクリプト": "set -eux\\n\\necho Hello from Controller!\\nsleep 10\\necho ''done.''"
    }',
    'system-admin',
    '2026-06-01 00:00:00.000'
WHERE NOT EXISTS (
    SELECT 1
    FROM job_history
    WHERE job_id = 1001 AND version = 2
);

UPDATE job_history
SET payload = '{
  "説明": "Lambda local fallback で実行される軽量なテストジョブ",
  "ジョブ名": "local-echo-job",
  "ジョブタイプ": "OneShot (単発)",
  "スケジュール (Cron)": "未設定",
  "有効化状態": "有効",
  "ワーカー定義": "default-local-lambda",
  "タイムアウト": "300 秒",
  "リトライ上限": 1,
  "バックオフ戦略": "固定時間",
  "初期遅延": "5 秒",
  "タグ": ["local", "test"],
  "直接設定の環境変数": {},
  "SSMパラメータ連携": {
    "リージョン": "",
    "パス": "",
    "再帰取得": "無効"
  },
  "Slack通知": {
    "ジョブ起動時": "無効",
    "成功時": "無効",
    "失敗時": "無効"
  },
  "スクリプト": "set -eux\\n\\necho Hello from Controller!\\nsleep 10\\necho ''done.''"
}'
WHERE job_id = 1001 AND version = 2;
