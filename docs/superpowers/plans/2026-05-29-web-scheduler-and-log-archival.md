# Web / Scheduler Split And Log Archival Plan

## Goal

Mrs. Harris を以下の構成へ段階的に寄せる。

1. 同一 image から `web` / `scheduler` の 2 role を起動できるようにする
2. `job_logs` は実行中専用の hot log buffer とし、終了後は archive store へ退避する
3. 将来的な Fargate 複数レプリカ運用に備えて、scheduler 系処理と log archive 処理を claim / idempotency 前提へ寄せる
4. `controller` worker type は deprecated 扱いにし、最終的に削除可能な状態へ進める

## Principles

- artifact は増やさず、同一 image の role 切替で運用する
- まず `web` / `scheduler` 分離を作り、その後に multi-replica 耐性を強める
- archive は「保存成功後に削除」の順を守る
- local / prod 差分は adapter と config に閉じ込める

## Phase 1: Runtime split

### Task 1. CLI role split

目的:

- `mrs-harris web`
- `mrs-harris scheduler`

の 2 起動モードを追加する。

作業:

- `main.rs` の CLI を role-aware にする
- `web` は HTTP/API/UI/WebSocket のみ起動
- `scheduler` は scheduler loop のみ起動
- 既存起動方法からの移行方針を整理

確認:

- `cargo run --bin mrs-harris -- web --config ...`
- `cargo run --bin mrs-harris -- scheduler --config ...`

### Task 2. App bootstrap split

目的:

- app 起動時に scheduler loop を無条件 spawn しないようにする

作業:

- `app.rs` / bootstrap の責務を整理
- `web` role では scheduler を起動しない
- `scheduler` role では HTTP listener を持たない、または最小化する

確認:

- `web` 単体で UI/API が動く
- `scheduler` 単体で pending run を dispatch できる

## Phase 2: Log archival foundation

### Task 3. Archive metadata migration

目的:

- `job_runs` に archive 状態を持たせる

候補カラム:

- `log_archive_status`
- `log_archive_store`
- `log_archive_key`
- `log_line_count`
- `log_archive_bytes`
- `log_archived_at`

確認:

- migration 適用
- Rust model / DB access / UI 読み分け影響確認

### Task 4. `LogArchiveStore` abstraction

目的:

- local/prod 差分を storage adapter に閉じ込める

実装:

- `LocalFileLogArchiveStore`
- skeleton としての `S3LogArchiveStore`

確認:

- local file archive round-trip test

### Task 5. Terminal run archive worker

目的:

- terminal run の `job_logs` を archive store へ退避する

作業:

- `pending -> exporting -> archived/failed` 状態遷移
- `SELECT ... FOR UPDATE` などで run を claim
- archive 成功後に `job_logs` delete

確認:

- export success path
- export failure path
- duplicate archive 実行が no-op になること

## Phase 3: UI read path split

### Task 6. Running vs archived read path

目的:

- 実行中は `job_logs`
- 終了後は archive store

の 2 経路を UI から透過的に使えるようにする

作業:

- run detail のログ取得 API / WebSocket 補助ロジックを整理
- `archived` は archive 読み込み
- `pending/exporting` は DB fallback
- `failed` は archive failure を表現

確認:

- running run
- archived run
- failed archive run

## Phase 4: Multi-scheduler safety

### Task 7. Scheduler work classification

目的:

- scheduler 内の処理を multi-replica 安全性で分類する

対象:

- cron trigger
- retry manager
- reaper
- log archive worker

成果物:

- 各処理ごとの claim / lease / idempotency 方針

### Task 8. Cron duplication guard

目的:

- 複数 scheduler で同じ cron run を二重作成しない

候補:

- run creation の unique key
- lease table
- scheduled slot claim

確認:

- 同時実行相当の integration test

### Task 9. Retry / reaper / archive claim safety

目的:

- terminal update, retry scheduling, archive 実行が複数 scheduler でも安全

確認:

- 同一 run への重複処理が破壊的にならない

## Phase 5: Worker-type transition

### Task 10. `controller` worker type deprecation

目的:

- `controller` worker type を本番経路から外す

作業:

- ドキュメントで deprecated 明記
- UI / worker definition 上での扱いを整理
- 本番では disable できる設定の検討

確認:

- local 開発ではまだ使える
- 本番寄り構成では `lambda / ecs` を優先できる

### Task 11. `floci` integration path

目的:

- ローカルで `S3 / Lambda / ECS` を検証可能にする

作業:

- `floci` 起動手順
- endpoint / credentials / bucket bootstrap
- lambda / ecs worker definition のローカル設定例

確認:

- archive to floci S3
- lambda callback
- ecs callback

## Recommended execution order

1. CLI role split
2. app bootstrap split
3. archive metadata migration
4. `LogArchiveStore`
5. terminal archive worker
6. archived log read path
7. cron duplication guard
8. retry/reaper/archive claim safety
9. `controller` worker type deprecation
10. `floci` integration docs / tests

## Risks

- role split 前に archive worker を入れると、どの process が何を担うか曖昧になりやすい
- cron duplication guard を後回しにしたまま scheduler 多重化すると二重 run 作成リスクが残る
- `controller` worker type を早く消しすぎると local dev の足場が不安定になる

## Deliverables

- `web` / `scheduler` role 分離
- archive metadata + archive worker
- archived log read path
- multi-scheduler safety design reflected in DB/claim logic
- `floci` ベースのローカル AWS 検証手順
