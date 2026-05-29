# Run Log Realtime Design

## Goal

ジョブ実行履歴詳細画面で、`controller / fargate / lambda` の 3 種類の worker type について、実行ログをリアルタイムに表示できるようにする。

既存の画面表示経路である `/runs/:id/logs/ws` と `job_logs` テーブルを活かしつつ、worker type ごとのログ取得差分を共通 abstraction で吸収する。

## Current State

- 画面側は [crates/controller/templates/runs/detail_live.html](/Users/shion.morikawa/private/mrs-harris/crates/controller/templates/runs/detail_live.html) で `/runs/:id/logs/ws` を購読している
- WebSocket 配信側は [crates/controller/src/web/runs.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/web/runs.rs) で `job_logs` をポーリングしている
- `controller` worker は Controller 内で `mrs_harris_worker::run_worker(...)` を直接起動している
- `fargate / lambda` は AWS 実行時に CloudWatch Logs が一次ログソースになる

結論として、UI を worker type ごとに分岐させるのではなく、すべて `job_logs` に正規化して既存 WebSocket へ載せるのが自然。

## Non-Goals

- DAG task ごとのログ設計見直し
- CloudWatch Logs を直接 UI に露出すること
- WebSocket の全面刷新
- ログ全文検索や永続保管ポリシーの再設計

## Requirements

### Functional

1. `controller` worker は実行中の stdout/stderr を逐次 `job_logs` に保存する
2. `fargate` worker は CloudWatch Logs からログを継続取得し、逐次 `job_logs` に保存する
3. `lambda` worker も CloudWatch Logs からログを継続取得し、逐次 `job_logs` に保存する
4. UI は既存 `/runs/:id/logs/ws` のままで、保存済み `job_logs` をリアルタイム表示する
5. run 完了後も少し遅れて到着するログを取りこぼさない
6. 同一ログの重複保存を防ぐ

### Non-Functional

1. worker type 差分を UI に漏らさない
2. CloudWatch 依存ロジックを `fargate / lambda` 個別実装へ散らしすぎない
3. ローカル fallback 実行でも同じ抽象を使う
4. 将来 DAG task logs を入れるときに、同じ収集 abstraction を流用しやすい構造にする

## Recommended Architecture

### Unified Pipeline

すべてのログ表示を次の 3 層で統一する。

1. `LogSource`
   - worker type ごとの一次ログ取得
2. `LogIngestion`
   - 取得したログを共通 `LogLine` へ正規化し、重複除去しつつ `job_logs` に append
3. `LogStreaming`
   - 既存 `/runs/:id/logs/ws` が `job_logs` を配信

```text
worker stdout / CloudWatch Logs
  -> LogSource
  -> normalized LogLine
  -> LogIngestion
  -> job_logs
  -> /runs/:id/logs/ws
  -> run detail UI
```

### Core Abstractions

#### 1. `RunLogCollector`

run ごとのログ収集 lifecycle を統括する coordinator。

想定責務:

- run と worker tracking 情報から適切な `LogSource` を選ぶ
- 非同期収集タスクの開始
- 終了条件の管理
- drain window の管理
- エラー時のログ出力

想定 API:

```rust
pub struct RunLogCollector;

impl RunLogCollector {
    pub async fn spawn_for_run(state: AppState, run: JobRun) -> anyhow::Result<()>;
}
```

#### 2. `LogSource` trait

worker type ごとの差分を隠蔽する主 abstraction。

```rust
#[async_trait::async_trait]
pub trait LogSource: Send {
    async fn next_batch(&mut self) -> anyhow::Result<LogBatch>;
    async fn is_exhausted(&self) -> anyhow::Result<bool>;
}
```

`LogBatch`:

- `lines: Vec<LogLine>`
- `cursor: Option<String>`
- `has_more: bool`

実装候補:

- `ControllerProcessLogSource`
- `CloudWatchLogSource`

ポイント:

- `fargate / lambda` で CloudWatch 読み出しの共通化を優先する
- worker type ごとの差は「stream 解決の方法」に寄せる

#### 3. `CloudWatchStreamLocator`

CloudWatch Logs を使う worker type 向けの補助 abstraction。

```rust
#[async_trait::async_trait]
pub trait CloudWatchStreamLocator: Send + Sync {
    async fn resolve(
        &self,
        state: &AppState,
        run: &JobRun,
    ) -> anyhow::Result<CloudWatchLogTarget>;
}
```

`CloudWatchLogTarget`:

- `log_group_name`
- `log_stream_name` or `stream_prefix`
- `region`
- `start_time_hint`

実装候補:

- `FargateStreamLocator`
- `LambdaStreamLocator`

設計意図:

- `CloudWatchLogSource` 自体は「既知の group/stream を読むだけ」に寄せる
- `fargate / lambda` の差分は locator に閉じ込める

#### 4. `LogSink`

`job_logs` への保存を一箇所に寄せる。

```rust
pub struct JobLogSink;

impl JobLogSink {
    pub async fn append_batch(
        pool: &MySqlPool,
        run_id: i64,
        batch: LogBatch,
    ) -> anyhow::Result<AppendResult>;
}
```

責務:

- `LogLine` の保存
- 重複除去
- `logged_at` の整形
- stream 種別の正規化

## Worker-Type Strategy

### Controller

最短経路で実現する。

- `mrs_harris_worker` の stdout/stderr キャプチャ時点で、行バッファを callback できるよう拡張
- 実行完了後まとめて callback するのではなく、行単位または短バッチ単位で `job_logs` へ送る
- これにより CloudWatch を介さず最小遅延で反映できる

必要変更の方向:

- `crates/worker/src/executor.rs`
- `crates/worker/src/log_capture.rs`
- `mrs_harris_worker::run_worker(...)` 周辺に streaming callback を追加

### Fargate

AWS 本番では CloudWatch Logs をログソースにする。

設計:

1. `launch_aws_fargate(...)` 後に `RunLogCollector::spawn_for_run(...)`
2. `FargateStreamLocator` が worker definition config と ECS task 情報から
   - `region`
   - `log_group_name`
   - `log_stream_name`
   を確定
3. `CloudWatchLogSource` が `GetLogEvents` 相当で増分取得
4. `JobLogSink` が `job_logs` へ append

worker definition に必要な設定候補:

- `aws_region`
- `log_group_name`
- `log_stream_prefix`
- 必要なら `container_name`

補足:

- ECS task ARN と stream 名の対応は設定解決を安定化させる必要がある
- task 起動直後は stream 未作成のことがあるので、初期リトライが必要

### Lambda

Lambda も CloudWatch Logs ベースで扱う。

設計:

1. `launch_aws_lambda(...)` 後に `RunLogCollector::spawn_for_run(...)`
2. `LambdaStreamLocator` が
   - `function_name`
   - `request_id`
   - `log_group_name`
   を元に stream 候補を解決
3. `CloudWatchLogSource` が増分取得
4. `JobLogSink` が `job_logs` へ append

worker definition に必要な設定候補:

- `aws_region`
- `log_group_name`
- `function_name`

補足:

- Lambda は stream 名解決が Fargate より曖昧になりやすい
- 初期版では `request_id` を含むイベント抽出戦略を明文化する

## Data Model Changes

既存 `job_logs` だけで足りるかをまず優先し、不足時のみ最小追加に留める。

追加候補:

1. `job_logs.external_event_id`
   - CloudWatch event の重複防止キー
2. `job_logs.source`
   - `controller | cloudwatch`
3. `workers.metadata`
   - log group / stream 解決結果や cursor を保存する余地

推奨:

- 初期版は `job_logs.external_event_id` を最優先で追加検討
- cursor 永続化は別テーブルより `workers.metadata` か専用軽量テーブルで管理

## Lifecycle

### Start

- run が `pending -> running` に向かうタイミングで collector 起動
- `controller` は worker 実行と同時
- `fargate/lambda` は launch 成功直後に起動し、stream 出現待ちを許容

### Steady State

- 1-3 秒程度の短周期で増分取得
- 空応答でも collector は継続
- UI は既存 WebSocket のまま

### Stop

collector は次を満たしたら終了:

1. run が terminal status
2. source が exhausted
3. drain window 経過

drain window 推奨:

- `controller`: 2-5 秒
- `fargate/lambda`: 10-30 秒

## Error Handling

- CloudWatch API 一時失敗は retry
- stream 未作成は warning にして一定期間再試行
- collector 自体の失敗は `tracing::error!` に出す
- UI は WebSocket 断ではなく、単に新しい `job_logs` が来ない状態として扱う

## Verification Plan

### Controller

- 長時間 `sleep` を含むローカル job で、実行中にログが 1 行ずつ増えること

### Fargate

- AWS 実環境またはモックで CloudWatch から増分取り込み
- stream 出現待ちと遅延到着ログの確認

### Lambda

- 非同期 invoke 後に request_id ベースでログが追えること

### UI

- 実行履歴詳細画面でページリロードなしにログが増えること
- 完了後も既存ログが残ること
- worker type に依らず同じ UI で読めること

## Open Decisions

1. CloudWatch cursor を DB に永続化するか、collector プロセス内メモリだけで始めるか
2. Lambda stream 解決を request_id ベースにするか、function log group 全体の time-window scan にするか
3. `job_logs` に dedupe key を持たせるか、collector 側の memory cache で済ませるか

## Recommendation

初期実装は次の順で進める。

1. `controller` の逐次ログ保存
2. `RunLogCollector / LogSource / JobLogSink` の共通 abstraction 導入
3. `CloudWatchLogSource` 共通化
4. `FargateStreamLocator`
5. `LambdaStreamLocator`

これにより、最初に価値の高い `controller` を短く完成させつつ、`fargate / lambda` を設計負債なく拡張できる。
