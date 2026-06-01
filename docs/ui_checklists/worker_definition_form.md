# UI Checklist: ワーカー定義作成・編集画面

このファイルはワーカー定義作成・編集画面 (`/worker-definitions/new`, `/worker-definitions/:id/edit`) における表示レベル（UI）の要件を管理するチェックリストです。修正を行った際は、対象の要件を満たしているかを検証してください。

## バックエンド種別
- [x] **Controller ワーカー種別の除去**:
  ワーカーのバックエンド種別の選択肢に `controller` が表示されず、`fargate` と `lambda` だけを選択できること。
- [x] **ローカル検証導線の表示**:
  ローカル検証が必要な場合は `lambda.function_name = "local"` または `fargate.cluster_arn = "local"` を使う旨が、フォーム上の補足で分かること。
