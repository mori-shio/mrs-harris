# Mrs. Harris (ミセス・ハリス) — サーバーレス分散ジョブスケジューラ

Mrs. Harris は、従来の Jenkins を代替することを目指して開発された、**Rust製のサーバーレス分散ジョブスケジューラシステム**です。
単一バイナリによるシンプルな構成でありながら、コントローラーとワーカーの協調による分散実行、AWS Fargate / AWS Lambda を活用したサーバーレスジョブのスケジューリングを強みとしています。

---

## 主な機能

- **単一バイナリ統合**: コントローラーとワーカーを1つのバイナリに統合。起動引数（サブコマンド）の切り替えのみで動作します。
- **サーバーレスワーカー**: ジョブ実行のバックエンドとして AWS Fargate および AWS Lambda をサポートし、リソースの動的な起動と自動シャットダウンを実現。
- **MySQLによる永続化**: 全てのジョブ定義、実行履歴、タスク実行結果、詳細ログを MySQL に一元化し永続化します。
- **DAG実行エンジン**: 依存関係を持つ複雑なタスク群を解析し、並列実行と順序制御をインテリジェントに行います。
- **高機能通知システム**: Slack Webhook や Email (SMTP) を通じて、ジョブの成功・失敗・デッドレター移行といったイベントをリアルタイムに美しく通知。
- **プレミアム Web UI**:
  - HSL調整されたダーク・グラスモーフィズムデザイン。
  - HTMXによる10秒間隔の自動ステータス更新。
  - **カレンダー画面 (`/calendar`)**: FullCalendar.jsを統合し、ヒートマップ風のジョブ履歴を表示。
  - **リアルタイムログ表示**: WebSocketを用いた自動スクロール付きのストリーミングログビューア。

---

## ディレクトリ構成

```text
mrs-harris/
├── Cargo.toml               # ワークスペース定義
├── Dockerfile               # マルチステージビルドDockerfile
├── docker-compose.yml       # ローカル動作検証環境 (MySQL + Controller)
├── config/                  # 設定ディレクトリ
│   ├── controller.toml      # ローカル用設定サンプル
│   └── controller-docker.toml # コンテナ用設定
├── crates/
│   ├── common/              # 共有型定義、設定、エラー型
│   ├── controller/          # スケジューラ本体、APIサーバー、Web UI
│   └── worker/              # シェルコマンド実行、ログキャプチャ、コールバック処理
├── static/                  # 静的アセット (CSS/JS)
└── templates/               # Askama テンプレート (HTML)
```

---

## クイックスタート (ローカル開発環境の立ち上げ)

もっとも簡単に Mrs. Harris を試すには、Docker Compose を使用します。

### 1. リポジトリのビルドと起動
以下のコマンドで、MySQLデータベースと Mrs. Harris コントローラーが自動的にビルドされ、立ち上がります。

```bash
docker-compose up --build
```

起動後、ブラウザで以下のアドレスにアクセスしてください：
- **Web UI URL**: `http://localhost:8080`
- **初期ログインアカウント**:
  - **ユーザー名**: `admin`
  - **パスワード**: `admin` (ローカル検証用)

---

## ジョブ定義 TOML ファイルの書き方

Mrs. Harris は、TOML ファイル形式でのジョブ定義インポートをサポートしています。

### 例1: 定期実行ジョブ (`cron_backup.toml`)
```toml
[job]
name = "Daily DB Backup"
description = "毎日深夜にデータベースのバックアップを実行するジョブ"
job_type = "cron"
schedule = "0 0 2 * * *"
worker_type = "fargate"
payload = { command = "mysqldump", args = ["-u", "mrs_harris", "-p", "mrs_harris", ">", "/backup/db.sql"] }

[job.retry]
max_retries = 3
backoff = "exponential"
base_delay_sec = 10

[job.timeout]
seconds = 3600

[job.notifications]
channels = ["Slack Alert", "Admin Email"]
on_events = ["failed", "dead_letter"]
```

### 例2: DAG（依存タスク）ジョブ (`etl_pipeline.toml`)
複数のタスクを順序立てて実行し、依存解決を行います。

```toml
[job]
name = "Daily ETL Pipeline"
description = "データの抽出・変換・ロードを行うパイプライン"
job_type = "dag"
worker_type = "fargate"

[[job.tasks]]
name = "extract_users"
worker_type = "lambda"
payload = { command = "python3", args = ["extract.py", "--type=users"] }

[[job.tasks]]
name = "extract_orders"
worker_type = "lambda"
payload = { command = "python3", args = ["extract.py", "--type=orders"] }

[[job.tasks]]
name = "transform_data"
worker_type = "fargate"
depends_on = ["extract_users", "extract_orders"]
payload = { command = "spark-submit", args = ["transform.py"] }

[[job.tasks]]
name = "load_warehouse"
worker_type = "fargate"
depends_on = ["transform_data"]
payload = { command = "spark-submit", args = ["load.py"] }
```

### ジョブのインポート方法
起動中のコントローラーコンテナ、またはローカルビルドされたバイナリから直接インポートできます。

```bash
# ローカルバイナリで実行する場合
cargo run --bin mrs-harris -- import --file config/examples/cron_backup.toml
```
同名のジョブが存在する場合は、自動的に上書き（カスケード削除の後に再作成）されます。

---

## 開発とテスト

### ユニットテスト・統合テストの実行
```bash
cargo test --workspace
```

### コード品質チェック
```bash
cargo clippy --workspace -- -D warnings
cargo fmt --check --all
```
