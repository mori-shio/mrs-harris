# Browser Dialog/Notice Unification Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace browser-native `alert()`, `confirm()`, and `hx-confirm` usage across the Controller UI with shared in-app dialog, toast, and inline notice components.

**Architecture:** Add shared `AppDialog`, `AppToast`, and `InlineNotice` primitives in `base.html` and `style.css`, then migrate existing pages to declarative hooks (`data-confirm-*`) and explicit success/error rendering. Keep destructive confirmations centralized, use toasts for success responses, and keep workflow-local validation errors inline near the affected controls.

**Tech Stack:** Askama templates, HTMX 1.9, vanilla JavaScript in `base.html`, shared CSS in `static/css/style.css`, browser regression scripts with `puppeteer-core`.

---

## File Map

- Modify: `crates/controller/templates/base.html`
  - Add dialog root, toast region, common JS APIs, HTMX hooks, focus/scroll handling, and confirm interception.
- Modify: `static/css/style.css`
  - Add shared dialog, toast, and inline notice styles and tone variants.
- Modify: `crates/controller/templates/jobs/detail.html`
  - Add inline notice host for history compare errors and remove browser-native alert fallback.
- Modify: `crates/controller/templates/jobs/list_partial.html`
  - Replace run-trigger alert with toast flow.
- Modify: `crates/controller/templates/runs/detail_live.html`
  - Replace cancel-request alert with toast flow.
- Modify: `crates/controller/templates/worker_definitions/detail.html`
  - Replace inline `confirm()` submit gate with declarative confirm attributes.
- Modify: `crates/controller/templates/spaces/detail.html`
  - Replace delete `confirm()` and run-trigger alert.
- Modify: `crates/controller/templates/spaces/list.html`
  - Replace delete `confirm()`.
- Create: `test_browser_dialog_notice.js`
  - Browser regression script covering dialog, toast, and inline notice flows.

### Task 1: Build Shared Dialog/Toast/Inline Notice Infrastructure

**Files:**
- Create: `test_browser_dialog_notice.js`
- Modify: `crates/controller/templates/base.html`
- Modify: `static/css/style.css`

- [ ] **Step 1: Write the failing browser test for shared UI roots**

```js
const puppeteer = require('puppeteer-core');

(async () => {
  const browser = await puppeteer.launch({
    executablePath: '/Applications/Google Chrome.app/Contents/MacOS/Google Chrome',
    headless: 'new'
  });
  const page = await browser.newPage();

  await page.goto('http://127.0.0.1:8080/login');
  await page.type('input[name="username"]', 'admin');
  await page.type('input[name="password"]', 'password');
  await Promise.all([
    page.waitForNavigation(),
    page.click('button[type="submit"]')
  ]);

  await page.goto('http://127.0.0.1:8080/jobs');
  const state = await page.evaluate(() => ({
    hasDialogRoot: !!document.getElementById('app-dialog-root'),
    hasToastRegion: !!document.getElementById('app-toast-region'),
    hasConfirmFn: typeof window.appConfirm,
    hasToastFn: typeof window.appToast,
    hasInlineFn: typeof window.renderInlineNotice
  }));

  if (!state.hasDialogRoot || !state.hasToastRegion) {
    throw new Error(JSON.stringify(state));
  }

  await browser.close();
})();
```

- [ ] **Step 2: Run the shared UI test and verify it fails**

Run: `node test_browser_dialog_notice.js`

Expected: FAIL with missing `app-dialog-root`, `app-toast-region`, or missing `window.appConfirm`.

- [ ] **Step 3: Add the shared dialog and toast markup to `base.html`**

```html
<main class="main-content">
    {% block content %}{% endblock %}
</main>

<div id="app-dialog-root"
     class="app-dialog-overlay"
     aria-hidden="true">
    <div class="app-dialog"
         role="dialog"
         aria-modal="true"
         aria-labelledby="app-dialog-title"
         aria-describedby="app-dialog-message">
        <div class="app-dialog-header">
            <h3 id="app-dialog-title"></h3>
            <button type="button" class="app-dialog-close" data-dialog-close>&times;</button>
        </div>
        <p id="app-dialog-message" class="app-dialog-message"></p>
        <div class="app-dialog-actions">
            <button type="button" class="btn btn-secondary" data-dialog-cancel>キャンセル</button>
            <button type="button" class="btn btn-danger" data-dialog-confirm>実行</button>
        </div>
    </div>
</div>

<div id="app-toast-region" aria-live="polite" aria-atomic="false"></div>
```

- [ ] **Step 4: Add the shared CSS primitives in `style.css`**

```css
.app-dialog-overlay {
    position: fixed;
    inset: 0;
    display: flex;
    align-items: center;
    justify-content: center;
    background: rgba(3, 7, 18, 0.78);
    backdrop-filter: blur(10px);
    opacity: 0;
    pointer-events: none;
    transition: var(--transition-smooth);
    z-index: 1200;
}

.app-dialog-overlay.active {
    opacity: 1;
    pointer-events: auto;
}

.app-dialog {
    width: min(560px, calc(100vw - 32px));
    border: 1px solid rgba(255,255,255,0.08);
    border-radius: var(--radius-lg);
    background: rgba(15, 23, 42, 0.96);
    box-shadow: 0 24px 48px rgba(0,0,0,0.45);
    padding: 24px;
}

#app-toast-region {
    position: fixed;
    top: 20px;
    right: 20px;
    display: flex;
    flex-direction: column;
    gap: 10px;
    z-index: 1250;
}

.app-toast {
    min-width: 280px;
    max-width: 420px;
    border-radius: var(--radius-md);
    border: 1px solid rgba(255,255,255,0.08);
    padding: 12px 14px;
    background: rgba(15, 23, 42, 0.95);
}

.inline-notice {
    display: flex;
    align-items: start;
    gap: 10px;
    border-radius: var(--radius-md);
    border: 1px solid rgba(255,255,255,0.08);
    padding: 12px 14px;
    margin-bottom: 14px;
}
```

- [ ] **Step 5: Add the shared JS APIs and HTMX confirm interception in `base.html`**

```js
window.appUi = {
  activeDialogTrigger: null,
  dialogResolver: null
};

window.appConfirm = function(options) {
  const overlay = document.getElementById('app-dialog-root');
  const title = document.getElementById('app-dialog-title');
  const message = document.getElementById('app-dialog-message');
  const confirmBtn = overlay.querySelector('[data-dialog-confirm]');
  const cancelBtn = overlay.querySelector('[data-dialog-cancel]');

  title.textContent = options.title || '確認';
  message.textContent = options.message || '';
  confirmBtn.textContent = options.confirmLabel || '実行';
  cancelBtn.textContent = options.cancelLabel || 'キャンセル';
  overlay.classList.add('active');
  overlay.setAttribute('aria-hidden', 'false');
  document.body.style.overflow = 'hidden';

  return new Promise(resolve => {
    window.appUi.dialogResolver = resolve;
    setTimeout(() => confirmBtn.focus(), 0);
  });
};

window.closeAppDialog = function(confirmed) {
  const overlay = document.getElementById('app-dialog-root');
  overlay.classList.remove('active');
  overlay.setAttribute('aria-hidden', 'true');
  document.body.style.overflow = '';
  if (window.appUi.dialogResolver) {
    window.appUi.dialogResolver(!!confirmed);
    window.appUi.dialogResolver = null;
  }
};

window.appToast = function(options) {
  const region = document.getElementById('app-toast-region');
  const toast = document.createElement('div');
  toast.className = 'app-toast app-toast-' + (options.tone || 'success');
  toast.textContent = options.message;
  region.appendChild(toast);
  setTimeout(() => toast.remove(), options.timeoutMs || 4000);
};

window.renderInlineNotice = function(container, options) {
  if (!container) return;
  let host = container.querySelector('.inline-notice-host');
  if (!host) {
    host = document.createElement('div');
    host.className = 'inline-notice-host';
    container.prepend(host);
  }
  host.innerHTML = '<div class="inline-notice inline-notice-' + (options.tone || 'info') + '" role="alert">' + options.message + '</div>';
};

document.addEventListener('click', async event => {
  const trigger = event.target.closest('[data-confirm-message]');
  if (!trigger || trigger.dataset.confirmPending === 'true') return;
  event.preventDefault();
  trigger.dataset.confirmPending = 'true';
  const confirmed = await window.appConfirm({
    title: trigger.dataset.confirmTitle,
    message: trigger.dataset.confirmMessage,
    confirmLabel: trigger.dataset.confirmConfirmLabel,
    cancelLabel: trigger.dataset.confirmCancelLabel,
    tone: trigger.dataset.confirmTone
  });
  trigger.dataset.confirmPending = 'false';
  if (!confirmed) return;
  if (trigger.dataset.confirmKind === 'form') {
    trigger.closest('form').submit();
    return;
  }
  if (trigger.dataset.confirmKind === 'htmx' && window.htmx) {
    htmx.trigger(trigger, 'confirmed-action');
  }
});
```

- [ ] **Step 6: Run the shared UI test and verify it passes**

Run: `node test_browser_dialog_notice.js`

Expected: PASS with no thrown error.

- [ ] **Step 7: Commit the shared infrastructure**

```bash
git add test_browser_dialog_notice.js crates/controller/templates/base.html static/css/style.css
git commit -m "feat: add shared dialog and toast primitives"
```

### Task 2: Migrate Destructive Confirmations to AppDialog

**Files:**
- Modify: `test_browser_dialog_notice.js`
- Modify: `crates/controller/templates/worker_definitions/detail.html`
- Modify: `crates/controller/templates/spaces/detail.html`
- Modify: `crates/controller/templates/spaces/list.html`
- Modify: `crates/controller/templates/jobs/detail.html`

- [ ] **Step 1: Extend the browser test with a destructive confirmation check**

```js
await page.goto('http://127.0.0.1:8080/jobs/local-echo-job');
const deleteButton = await page.$('[data-confirm-message*="ジョブ定義"]');
if (!deleteButton) {
  throw new Error('job delete button is not using data-confirm-message');
}
await deleteButton.click();
const dialogState = await page.evaluate(() => ({
  active: document.getElementById('app-dialog-root').classList.contains('active'),
  title: document.getElementById('app-dialog-title').textContent
}));
if (!dialogState.active) {
  throw new Error(JSON.stringify(dialogState));
}
```

- [ ] **Step 2: Run the confirmation test and verify it fails**

Run: `node test_browser_dialog_notice.js`

Expected: FAIL because target buttons/forms still use `confirm()` or `hx-confirm`.

- [ ] **Step 3: Replace `hx-confirm` on job delete with declarative confirm attributes**

```html
<button class="btn btn-danger"
        hx-delete="/api/jobs/{{ job.name }}"
        hx-trigger="confirmed-action"
        data-confirm-kind="htmx"
        data-confirm-title="ジョブを削除しますか？"
        data-confirm-message="このジョブ定義を削除します。元に戻せません。"
        data-confirm-confirm-label="削除する"
        data-confirm-cancel-label="キャンセル"
        data-confirm-tone="danger"
        hx-headers='{"Authorization": ""}'
        hx-on::after-request="if (event.detail.successful) window.location.href='/jobs'">
```

- [ ] **Step 4: Replace inline `confirm()` forms with declarative confirm attributes**

```html
<form action="/spaces/{{ space.id }}/delete"
      method="POST"
      data-confirm-kind="form"
      data-confirm-title="スペースを削除しますか？"
      data-confirm-message="所属していたジョブは未分類へ移動します。"
      data-confirm-confirm-label="削除する"
      data-confirm-cancel-label="キャンセル"
      data-confirm-tone="danger">
    <button type="submit" class="btn btn-secondary">削除</button>
</form>
```

```html
<form action="/worker-definitions/{{ def.id }}/delete"
      method="POST"
      data-confirm-kind="form"
      data-confirm-title="ワーカー定義を削除しますか？"
      data-confirm-message="紐付いているジョブの実行に影響が出る可能性があります。"
      data-confirm-confirm-label="削除する"
      data-confirm-cancel-label="キャンセル"
      data-confirm-tone="danger">
```

- [ ] **Step 5: Run the confirmation test and verify it passes**

Run: `node test_browser_dialog_notice.js`

Expected: PASS, and opening delete actions activates `#app-dialog-root` without a native browser prompt.

- [ ] **Step 6: Commit the confirm migration**

```bash
git add test_browser_dialog_notice.js crates/controller/templates/worker_definitions/detail.html crates/controller/templates/spaces/detail.html crates/controller/templates/spaces/list.html crates/controller/templates/jobs/detail.html
git commit -m "feat: replace native confirms with app dialog"
```

### Task 3: Migrate Success Feedback to AppToast

**Files:**
- Modify: `test_browser_dialog_notice.js`
- Modify: `crates/controller/templates/jobs/list_partial.html`
- Modify: `crates/controller/templates/spaces/detail.html`
- Modify: `crates/controller/templates/runs/detail_live.html`
- Modify: `crates/controller/templates/jobs/detail.html`

- [ ] **Step 1: Extend the browser test with a toast assertion**

```js
await page.goto('http://127.0.0.1:8080/jobs');
await page.click('button[hx-post="/api/jobs/local-echo-job/trigger"]');
await page.waitForFunction(() => {
  const region = document.getElementById('app-toast-region');
  return region && region.textContent.includes('ジョブ実行をトリガーしました');
});
```

- [ ] **Step 2: Run the toast test and verify it fails**

Run: `node test_browser_dialog_notice.js`

Expected: FAIL because no toast appears and the current implementation still relies on `alert()`.

- [ ] **Step 3: Replace list/detail success alerts with toast hooks**

```html
<button class="btn btn-secondary"
        hx-post="/api/jobs/{{ job.name }}/trigger"
        hx-swap="none"
        hx-on::after-request="if (event.detail.successful) window.appToast({ message: 'ジョブ実行をトリガーしました。ダッシュボードで確認してください。', tone: 'success' })">
```

```html
<button class="btn btn-danger"
        hx-post="/api/runs/{{ run.id }}/cancel"
        hx-swap="none"
        hx-on::after-request="if (event.detail.successful) window.appToast({ message: '実行キャンセルをリクエストしました。', tone: 'success' })">
```

```html
<button class="btn btn-secondary"
        hx-post="/api/jobs/{{ job.name }}/trigger"
        hx-swap="none"
        hx-on::after-request="if (event.detail.successful) { startRunPolling(); window.appToast({ message: 'ジョブ実行をトリガーしました。', tone: 'success' }); }">
```

- [ ] **Step 4: Run the toast test and verify it passes**

Run: `node test_browser_dialog_notice.js`

Expected: PASS, with toast text visible in `#app-toast-region` and no native alert.

- [ ] **Step 5: Commit the toast migration**

```bash
git add test_browser_dialog_notice.js crates/controller/templates/jobs/list_partial.html crates/controller/templates/spaces/detail.html crates/controller/templates/runs/detail_live.html crates/controller/templates/jobs/detail.html
git commit -m "feat: replace success alerts with app toast"
```

### Task 4: Migrate History Compare Validation to Inline Notice

**Files:**
- Modify: `test_browser_dialog_notice.js`
- Modify: `crates/controller/templates/base.html`
- Modify: `crates/controller/templates/jobs/detail.html`

- [ ] **Step 1: Extend the browser test for history compare inline notice**

```js
await page.goto('http://127.0.0.1:8080/jobs/local-echo-job?tab=history');
await page.click('input[name="compare-a"]:checked');
await page.click('button[onclick="compareVersions()"]');
await page.waitForFunction(() => {
  const pane = document.getElementById('tab-history');
  return pane && pane.textContent.includes('比較する2つのバージョン');
});
const noticeState = await page.evaluate(() => ({
  hasInlineNotice: !!document.querySelector('#history-inline-notice-host .inline-notice'),
  bodyOverflow: document.body.style.overflow
}));
if (!noticeState.hasInlineNotice || noticeState.bodyOverflow === 'hidden') {
  throw new Error(JSON.stringify(noticeState));
}
```

- [ ] **Step 2: Run the inline notice test and verify it fails**

Run: `node test_browser_dialog_notice.js`

Expected: FAIL because `compareVersions()` still falls back to the alert path.

- [ ] **Step 3: Add a dedicated inline notice host to the history card**

```html
<div id="tab-history" class="ec2-tab-pane">
    <div class="card" style="padding:24px;">
        <div id="history-inline-notice-host"></div>
        <div style="display:flex;justify-content:space-between;align-items:center;margin-bottom:16px;">
```

- [ ] **Step 4: Replace the compare validation alert with `renderInlineNotice()`**

```js
window.compareVersions = function() {
  syncCompareStateFromDom();
  window.compareState.initialized = true;

  if (!window.compareState.a.version || !window.compareState.b.version) {
    const host = document.getElementById('history-inline-notice-host');
    window.renderInlineNotice(host, {
      message: '比較する2つのバージョン（AとB）を選択してください。',
      tone: 'warning'
    });
    return;
  }
  const host = document.getElementById('history-inline-notice-host');
  if (host) host.innerHTML = '';
  // existing diff rendering continues here
};
```

- [ ] **Step 5: Run the inline notice test and verify it passes**

Run: `node test_browser_dialog_notice.js`

Expected: PASS, with an inline notice rendered above the history controls and no modal/alert side effect.

- [ ] **Step 6: Commit the inline notice migration**

```bash
git add test_browser_dialog_notice.js crates/controller/templates/base.html crates/controller/templates/jobs/detail.html
git commit -m "feat: show history compare validation inline"
```

### Task 5: Accessibility, Cleanup, and End-to-End Verification

**Files:**
- Modify: `test_browser_dialog_notice.js`
- Modify: `crates/controller/templates/base.html`
- Modify: `static/css/style.css`

- [ ] **Step 1: Extend the browser test with keyboard and dismissal checks**

```js
await page.goto('http://127.0.0.1:8080/jobs/local-echo-job');
await page.click('[data-confirm-message*="削除"]');
await page.keyboard.press('Escape');
const closedByEsc = await page.evaluate(() => !document.getElementById('app-dialog-root').classList.contains('active'));
if (!closedByEsc) {
  throw new Error('dialog did not close on Escape');
}
```

- [ ] **Step 2: Run the accessibility test and verify it fails**

Run: `node test_browser_dialog_notice.js`

Expected: FAIL if Escape, focus restore, or dismiss behavior is incomplete.

- [ ] **Step 3: Finish dialog focus and dismissal handling in `base.html`**

```js
document.addEventListener('keydown', event => {
  const overlay = document.getElementById('app-dialog-root');
  if (!overlay.classList.contains('active')) return;
  if (event.key === 'Escape') {
    event.preventDefault();
    window.closeAppDialog(false);
  }
});

document.getElementById('app-dialog-root').addEventListener('click', event => {
  if (event.target.id === 'app-dialog-root') {
    window.closeAppDialog(false);
  }
});

document.querySelector('[data-dialog-cancel]').addEventListener('click', () => window.closeAppDialog(false));
document.querySelector('[data-dialog-confirm]').addEventListener('click', () => window.closeAppDialog(true));
```

- [ ] **Step 4: Run the full browser regression script and manual browser QA**

Run: `node test_browser_dialog_notice.js`

Expected: PASS

Manual QA in the in-app browser:
- `/jobs/local-echo-job?tab=history` で A/B 未選択時に inline notice が出る
- `/jobs` と `/spaces/:id` でジョブ実行後に toast が出る
- `/spaces/:id`、`/spaces`、`/worker-definitions/:id`、`/jobs/:name` で削除確認が app dialog になる

- [ ] **Step 5: Commit the accessibility and verification pass**

```bash
git add test_browser_dialog_notice.js crates/controller/templates/base.html static/css/style.css
git commit -m "fix: finalize dialog accessibility and regression coverage"
```
