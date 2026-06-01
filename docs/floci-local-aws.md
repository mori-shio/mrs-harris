# floci を使ったローカル AWS 検証

Mrs. Harris の以下をローカルで検証するためのメモです。

- `S3` への実行ログアーカイブ
- `Lambda` worker callback
- `ECS/Fargate` worker callback

## 前提

- Docker が利用可能
- `floci` コンテナを `4566` で起動する

例:

```bash
docker run --rm -it \
  -p 4566:4566 \
  -v /var/run/docker.sock:/var/run/docker.sock \
  floci/floci:latest
```

## S3 検証

1. `floci` 上に archive bucket を作成する
2. `config/controller.toml` の `log_archive` を `s3` 向け設定へ切り替える
3. terminal run 完了後に `job_runs.log_archive_status = archived` となること
4. `job_logs` から当該 run の hot logs が削除されること
5. 実行詳細画面からアーカイブログを再表示できること

設定例:

```toml
[log_archive]
store = "s3"
s3_bucket = "mrs-harris-logs"
s3_prefix = "dev"
s3_region = "us-east-1"
s3_endpoint_url = "http://localhost:4566"
s3_force_path_style = true
```

`floci` / LocalStack 系の endpoint を使う場合は、`s3_force_path_style = true` を推奨する。

起動前に最低限のダミー認証情報を環境変数で渡す。

```bash
export AWS_ACCESS_KEY_ID=test
export AWS_SECRET_ACCESS_KEY=test
export AWS_REGION=us-east-1
```

bucket 作成例:

```bash
aws --endpoint-url http://127.0.0.1:4566 s3api create-bucket --bucket mrs-harris-logs
```

## Lambda worker 検証

1. `floci` 上に callback 付き Lambda を定義する
2. Worker Definition を `lambda` で作成する
3. 実行後、`job_runs` が `running -> terminal` に遷移すること
4. CloudWatch Logs 相当のログ取り込みが機能すること

## ECS/Fargate worker 検証

1. `floci` 上に ECS cluster / task definition を定義する
2. Worker Definition を `fargate` で作成する
3. callback と実行ログ収集が機能すること

## ローカル fallback の扱い

- `controller` worker type は廃止し、使わない
- ローカル開発では worker definition 側で以下を使う
  - Lambda: `{"function_name": "local"}`
  - Fargate: `{"cluster_arn": "local"}`
- これらの設定では AWS 呼び出しの代わりに Controller 内の local fallback 実行へ切り替わる
