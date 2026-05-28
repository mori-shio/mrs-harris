# Browser Dialog/Notice Unification Design

## Goal

Mrs. Harris UI からブラウザ標準の `alert()`、`confirm()`、`hx-confirm` 依存をなくし、画面内で統一された確認・通知・エラー表示へ置き換える。

対象はジョブ画面に限定せず、Controller の Web UI 全体とする。

## Scope

この設計が扱う対象:

- 破壊的操作の確認 UI
- 成功通知 UI
- 入力不足や業務エラーの表示 UI
- HTMX を含む画面遷移、フォーム送信、ボタン操作時の共通ハンドリング

この設計が扱わない対象:

- サーバー API のレスポンス仕様変更
- 認証フローの全面再設計
- WebSocket やリアルタイム通知基盤の導入

## Current State

現状のブラウザ依存 UI は以下に存在する。

### alert

- 履歴比較の未選択エラー: `crates/controller/templates/base.html`
- ジョブ実行トリガー通知: `crates/controller/templates/jobs/list_partial.html`
- スペース詳細からのジョブ実行トリガー通知: `crates/controller/templates/spaces/detail.html`
- 実行キャンセル要求送信通知: `crates/controller/templates/runs/detail_live.html`

### confirm

- ワーカー定義削除: `crates/controller/templates/worker_definitions/detail.html`
- スペース削除: `crates/controller/templates/spaces/detail.html`
- スペース一覧からの削除: `crates/controller/templates/spaces/list.html`

### hx-confirm

- ジョブ削除: `crates/controller/templates/jobs/detail.html`

### Reusable Assets

- 共通モーダル向け CSS: `static/css/style.css`
- 既存の差分比較モーダル: `crates/controller/templates/jobs/detail.html`
- 既存のバージョン詳細モーダル: `crates/controller/templates/jobs/detail.html`
- 既存のスペース作成モーダル: `crates/controller/templates/jobs/list.html`

## Design Principles

- 確認は dialog のみで扱う
- 成功通知は toast を標準にする
- 入力不足や業務エラーは inline notice を第一選択にする
- ブラウザ標準ダイアログを新規追加しない
- HTMX と通常フォーム送信の両方で同じ UX を保つ
- 画面ごとの独自実装ではなく、`base.html` 起点の共通基盤へ寄せる

## Component Model

### AppDialog

用途:

- 破壊的操作の確認
- ユーザーが明示的に Yes/No を選ぶべき操作

仕様:

- タイトル、本文、キャンセル、主アクションボタンを持つ
- `default` と `danger` のトーンを持つ
- Esc、背景クリック、右上クローズで閉じる
- フォーカストラップを持つ
- 表示中は背景スクロールをロックする
- `Promise<boolean>` で結果を返す

想定 API:

```js
window.appConfirm({
  title: "スペースを削除しますか",
  message: "所属ジョブは未分類へ移動します。",
  confirmLabel: "削除する",
  cancelLabel: "キャンセル",
  tone: "danger"
})
```

### AppToast

用途:

- 成功通知
- 軽量な情報通知

仕様:

- 画面右上または右下に縦積み
- `success`、`info`、`warning`、`error` のトーン
- 標準は `success`
- 3-5 秒で自動消滅
- 手動クローズ可能
- 連続発火時は積み上げる

想定 API:

```js
window.appToast({
  message: "ジョブ実行をトリガーしました。",
  tone: "success",
  timeoutMs: 4000
})
```

### InlineNotice

用途:

- 入力不足
- 業務エラー
- その場で読み取るべき補足情報

仕様:

- カード上部または対象エリア上部に表示
- `info`、`warning`、`error` のトーン
- 原則、自動消滅しない
- 同じコンテナでは最新の notice で置換する

想定 API:

```js
window.renderInlineNotice(container, {
  message: "比較する2つのバージョンを選択してください。",
  tone: "warning"
})
```

## Placement Rules

### Dialog

以下に限定して使う。

- スペース削除
- ワーカー定義削除
- ジョブ削除
- 今後追加される破壊的操作

### Toast

以下を標準対象とする。

- ジョブ実行トリガー成功
- 実行キャンセル要求送信成功
- 保存完了
- 作成完了
- 更新完了

### InlineNotice

以下を標準対象とする。

- 履歴比較の A/B 未選択
- フォーム入力不足
- 画面文脈に紐づく業務エラー

## Event Integration

### Confirm Flow

既存の `confirm()` と `hx-confirm` はそのまま使わず、属性ベースの共通ハンドラへ置き換える。

想定属性:

- `data-confirm-title`
- `data-confirm-message`
- `data-confirm-confirm-label`
- `data-confirm-cancel-label`
- `data-confirm-tone`
- `data-confirm-kind`

`data-confirm-kind` の値:

- `form`
- `htmx`
- `button`

動作:

1. クリックまたは submit 前に共通ハンドラが介入する
2. 元イベントは `preventDefault()` する
3. `appConfirm()` を開く
4. 確認された場合のみ、本来の submit または HTMX request を再実行する

### Toast Flow

成功 toast はフロント単独イベントと、サーバー往復後イベントの両方に対応する。

- 純フロント操作: その場で `appToast()` を呼ぶ
- HTMX / form 送信後: 応答後イベントで `appToast()` を呼ぶ

将来的には `HX-Trigger` またはレスポンス上の共通属性で通知を返せるように拡張可能とするが、初期導入ではテンプレート側の既存成功フローに合わせた最小実装でよい。

### Inline Notice Flow

各画面は notice を差し込む先のコンテナを明示する。

例:

- 履歴比較エラー: 履歴カード上部
- フォームエラー: フォームカード先頭
- 実行詳細の補足エラー: ライブ詳細のヘッダ直下

## Architecture

### Base Layer

`crates/controller/templates/base.html` に以下を追加する。

- dialog root
- toast region
- 共通 JavaScript API
- 共通イベントハンドラ登録

`static/css/style.css` に以下を追加または整理する。

- dialog styles
- toast stack styles
- inline notice styles
- 共通アニメーション

### Page Layer

各ページテンプレートは以下だけを持つ。

- `data-confirm-*` 属性
- toast 呼び出しに必要な最小 hook
- inline notice の配置先コンテナ

ページごとに独自の確認モーダル実装を増やさない。

## Migration Plan

### Phase 1: Shared Infrastructure

- `base.html` に `AppDialog`、`AppToast`、`InlineNotice` の基盤追加
- `style.css` に共通スタイル追加
- 共通 API とイベントバインディング追加

### Phase 2: Confirm Migration

以下を `appConfirm()` へ置換:

- スペース削除
- ワーカー定義削除
- ジョブ削除

### Phase 3: Toast Migration

以下を `appToast()` へ置換:

- ジョブ実行トリガー成功
- 実行キャンセル要求送信成功
- 保存や作成など、既存の成功通知

### Phase 4: Inline Notice Migration

以下を `renderInlineNotice()` へ置換:

- 履歴比較の A/B 未選択
- フォーム入力不足や業務エラー

## Accessibility Requirements

- Dialog 表示時は最初の操作要素へフォーカスを移す
- Dialog 内でフォーカスを循環させる
- Dialog を閉じたら元のトリガー要素へフォーカスを戻す
- Esc で閉じられる
- `role="dialog"` と `aria-modal="true"` を付与する
- Toast は読み上げを考慮し、`aria-live` を設定する
- Inline notice は `role="status"` または `role="alert"` を使い分ける

## Error Handling

- `appConfirm()` 実行中に対象要素が DOM から消えた場合は何もしない
- HTMX 再実行時に二重送信しないようガードを持つ
- Toast 表示に失敗しても本来の業務処理を止めない
- Inline notice の描画先が見つからない場合のみフォールバックとして `appToast({ tone: "error" })` を許容する

## Testing Strategy

### UI Verification

- Dialog が開閉する
- Toast が積み上がる
- Inline notice が対象領域に表示される
- 背景スクロールロックが効く

### Interaction Verification

- Dialog の OK / Cancel / Esc / 背景クリックが正しく動く
- HTMX 送信が確認後にのみ再実行される
- 二重クリックで重複送信しない
- Toast が自動消滅する

### Regression Verification

- スペース削除フロー
- ワーカー定義削除フロー
- ジョブ削除フロー
- ジョブ実行トリガーフロー
- 実行キャンセル要求フロー
- 履歴比較の未選択エラーフロー

## Risks

- `hx-confirm` から共通 confirm への移行で、HTMX の送信再実行を誤ると二重送信や無送信が起きる
- 既存テンプレートに inline `onclick` が多く、通知 API 置換の粒度を誤ると責務が分散しやすい
- Dialog と既存の画面内モーダルが競合すると、z-index やスクロールロックの衝突が起きる

## Recommendation

共通 UI 基盤を `base.html` と `style.css` に集約し、確認は `AppDialog`、成功通知は `AppToast`、入力不足や業務エラーは `InlineNotice` に分離する。

これにより、ブラウザ依存 UI を除去しつつ、HTMX 主体の既存構成へ最小変更で統一 UX を導入できる。
