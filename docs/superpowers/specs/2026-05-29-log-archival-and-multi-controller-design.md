# Log Archival And Multi-Controller Design

## Goal

以下を同時に満たす将来設計を定義する。

1. `job_logs` を実行中のホットログ置き場に限定し、実行終了後はアーカイブへ退避する
2. ローカル開発では `floci` を使って S3 / Lambda / ECS(Fargate 相当) の検証をしやすくする
3. 最終的に Harris Controller 自体を Fargate 上で複数レプリカ動作させる前提に耐える設計へ寄せる

この spec はまず設計方針を固めるものであり、S3 アーカイブ、multi-controller 対応、scheduler 分離、leader election の即時実装までは含まない。

## Current State

### Logs

- 実行中ログは `job_logs` に逐次保存される
- 実行詳細画面は `/runs/:id/logs/ws` 経由で `job_logs` をポーリング配信している
- `job_logs` は保持期間やアーカイブ方針がなく、将来的にサイズが無制限に増える

### Controller topology

- Controller 起動時に scheduler loop が常に起動する  
  [app.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/app.rs)
- scheduler は `cron_trigger`, `retry_manager`, `dispatcher`, `reaper` を順に回す  
  [scheduler/mod.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/scheduler/mod.rs)
- `dispatcher` は `claim_pending_run(... FOR UPDATE)` により比較的多重起動耐性がある  
  [dispatcher.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/scheduler/dispatcher.rs)  
  [runs.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/db/runs.rs)
- 一方で `cron_trigger` は全 Controller が同じ Cron ジョブを見て `create_run()` するため、複数 Controller で二重起動リスクがある  
  [cron_trigger.rs](/Users/shion.morikawa/private/mrs-harris/crates/controller/src/scheduler/cron_trigger.rs)

### Local AWS emulation

- `floci` は LocalStack 互換の AWS ローカルエミュレータとして、`S3`, `Lambda`, `ECS`, `CloudWatch Logs` をサポートしている  
  [floci homepage](https://floci.io/)  
  [floci overview](https://floci.io/floci/)  
  [migrate from LocalStack](https://floci.io/floci/getting-started/migrate-from-localstack/)  
  [floci GitHub](https://github.com/floci-io/floci)
- ドキュメント上、Lambda/ECS は Docker-backed execution とされており、Mrs. Harris の worker integration 検証と相性が良い

## Non-Goals

- ログ全文検索の提供
- CloudWatch Logs / S3 に対する高度な検索 UI
- すべての scheduler 問題の一括解決
- いきなり Controller 全体を fully stateless / horizontally scalable にすること

## Requirements

### Functional

1. 実行中 run のログは低遅延で UI に見えること
2. 実行終了後のログは DB から外しても閲覧できること
3. アーカイブ失敗時にログをロストしないこと
4. ローカル開発で本番相当の S3 / Lambda / ECS worker 動作確認ができること
5. 将来的な Controller 複数レプリカ化に向けて、scheduler 系の重複実行リスクを明確に分離できること

### Non-Functional

1. `job_logs` は長期保存の主ストアにしない
2. アーカイブ処理は冪等にする
3. 複数 Controller を前提にしても、同じ run のアーカイブや Cron 起動が重複しにくい設計にする
4. local と prod の差分は storage/infra adapter に閉じ込める

## Recommended Direction

## 1. `job_logs` を hot log buffer に限定する

役割を明確に分ける。

- `job_logs`
  - 実行中ログの一時保存
  - WebSocket / run detail のリアルタイム配信源
  - retention は短い
- Archive store
  - 終了済みログの長期保存
  - まずは `S3`

これにより DB の負荷を「ログ保管」から「実行中表示」に限定できる。

## 2. 終了後アーカイブフロー

推奨フロー:

1. run が terminal state へ遷移
2. `job_logs` を run 単位で export
3. archive store への保存成功
4. `job_runs` に archive metadata を保存
5. `job_logs` を削除

重要:

- `S3 保存成功 -> metadata 更新 -> DB 削除` の順にする
- 保存失敗時は `job_logs` を残す
- 削除は成功確認後のみ

## 3. Archive format

最初は `jsonl.gz` を推奨する。

例:

```text
s3://<bucket>/job-runs/<job_id>/<run_id>.jsonl.gz
```

各行:

```json
{"logged_at":"2026-05-29T04:33:13.155Z","stream":"stderr","task_name":null,"line":"+ echo 'いいね！'"}
```

理由:

- append/stream ではなく終了後 export と相性が良い
- gzip で容量を抑えやすい
- 1 行 1 レコードで復元しやすい

## 4. `job_runs` metadata extension

追加候補:

- `log_archive_status`
  - `pending`
  - `exporting`
  - `archived`
  - `failed`
- `log_archive_store`
  - `s3`
  - `local_file`
- `log_archive_key`
- `log_line_count`
- `log_archive_bytes`
- `log_archived_at`

これにより UI は `job_logs` が空でも「ログが失われた」のか「アーカイブ済み」なのかを判定できる。

## 5. Storage abstraction

推奨 trait:

```rust
pub trait LogArchiveStore: Send + Sync {
    async fn put_run_logs(&self, run: &JobRun, logs: &[LogLine]) -> anyhow::Result<ArchivePutResult>;
    async fn get_run_logs(&self, run: &JobRun) -> anyhow::Result<Vec<LogLine>>;
    async fn delete_run_logs(&self, run: &JobRun) -> anyhow::Result<()>;
}
```

実装:

- `S3LogArchiveStore`
- `LocalFileLogArchiveStore`

### Local dev

ローカルでは 2 段階で考える。

1. 最初の実装・テストは `LocalFileLogArchiveStore`
2. AWS worker 統合検証は `floci` の S3 を使う

この方針なら、local 開発を S3 エミュレータ必須にせず、必要な時だけ `floci` を上げればよい。

## 6. UI read path

### Running

- 従来どおり `job_logs` + `/runs/:id/logs/ws`

### Terminal

- `log_archive_status = archived`
  - archive store から読む
- `pending / exporting`
  - まだ `job_logs` を読む
- `failed`
  - `job_logs` が残っていればそこから読む
  - 残っていなければ archive failure を明示

この構成なら「終了後すぐ見に行ったらログが空」は避けられる。

## Multi-controller implications

## 1. 現状の評価

### 比較的安全

- `claim_pending_run(... FOR UPDATE)` ベースの dispatch

### 危険

- `cron_trigger`
- `retry_manager`
- `reaper`
- 将来の `log archive worker`

理由:

- 現状は Controller ごとに scheduler loop が常に起動する
- 複数 Fargate タスクで起動すると同じ処理が全台で走る

## 2. Recommended topology

最初の現実解:

- `web/controller` role: 複数台
- `scheduler` role: 1 台

つまり「Controller を複数台」にする時も、scheduler だけは別 role として 1 レプリカで動かす。

これが最短で安全。

## 3. Longer-term topology

将来的には DB lease / leader election を入れてもよい。

候補:

- MySQL の lease table
- `GET_LOCK`
- Redis lock

ただし最初は scheduler 分離の方が単純で壊れにくい。

## 4. Archive worker and multi-controller

ログアーカイブも multi-controller を前提に冪等化する必要がある。

推奨:

- `job_runs.log_archive_status = pending` の run を claim する専用処理
- `SELECT ... FOR UPDATE` で 1 台だけが `exporting` へ遷移
- `archived` 済みなら再実行しても no-op

つまり archive 処理も scheduler と同じく「claim-based worker」にする。

## `floci` usage recommendation

### What to use it for

1. `S3LogArchiveStore` の integration test
2. `lambda` worker callback / logs / archive の動作確認
3. `fargate(ECS)` worker callback / logs / archive の動作確認

### What not to over-assume

- 本番の IAM / networking / ENI の完全再現
- Fargate 本番性能特性

使いどころは「Mrs. Harris の AWS 依存統合テスト」であって、「AWS 本番完全再現」ではない。

## Implementation sequence

1. `LogArchiveStore` abstraction 追加
2. `LocalFileLogArchiveStore` 実装
3. `job_runs` archive metadata migration
4. terminal run export worker 実装
5. run detail の archive read path 追加
6. `S3LogArchiveStore` 実装
7. `floci` ベースのローカル integration 手順整備
8. scheduler role 分離 spec / implementation

## Open questions

1. `job_logs` を terminal 遷移直後すぐ削除するか、数分の grace period を置くか
2. archive format は `jsonl.gz` 固定でよいか
3. run detail で archive 読み込みを全文にするか、ページングを入れるか
4. Controller 複数台化の先に scheduler 分離だけで十分か、lease まで先に入れるか

## Recommendation Summary

結論:

- ログ検索をスコープ外にする前提なら、`job_logs` を hot log buffer に限定し、終了後は S3 へ逃がす設計は有効
- ローカル開発では `floci` を S3 / Lambda / ECS worker の統合検証に使う価値が高い
- Harris Controller 自体の Fargate 複数レプリカ化を見据えるなら、scheduler 系と log archive worker は「全台で動いてよい処理」ではない
- したがって、次の大きな設計原則は以下になる

1. ログは hot/cold を分離する
2. archive は冪等な claim-based worker にする
3. Controller web と scheduler を分離する
4. local/prod 差分は archive store adapter と AWS emulator adapter に閉じ込める
