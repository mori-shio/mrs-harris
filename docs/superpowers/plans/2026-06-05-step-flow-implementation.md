# StepFlow Implementation Plan

> **For agentic workers:** REQUIRED SUB-SKILL: Use superpowers:subagent-driven-development (recommended) or superpowers:executing-plans to implement this plan task-by-task. Steps use checkbox (`- [ ]`) syntax for tracking.

**Goal:** Replace DAG jobs with standalone StepFlow orchestration backed by its own tables, models, web UI, seed data, and execution records.

**Architecture:** Jobs remain worker-backed `OneShot`/`Cron` definitions. StepFlow is a separate controller-side orchestration domain with groups, steps, version history, parent runs, and step-run mappings to child `job_runs`.

**Tech Stack:** Rust, Axum 0.8, Askama, SQLx MySQL, HTMX, existing Mrs. Harris scheduler/callback patterns.

---

## Task 1: Core Types And DAG Type Removal

**Files:**
- Modify: `crates/common/src/models/job.rs`
- Modify: `crates/common/src/models/run.rs`
- Create: `crates/common/src/models/step_flow.rs`
- Modify: `crates/common/src/models/mod.rs`

- [ ] Add failing tests that assert `JobType::Dag` is no longer parseable and `TriggerType::StepFlow` renders as `step_flow`.
- [ ] Add StepFlow model types: `StepFlow`, `StepFlowGroup`, `StepFlowStep`, `StepFlowRun`, `StepFlowStepRun`, `StepFlowHistoryEntry`, `StepFlowRunCondition`.
- [ ] Move group run condition validation into `StepFlowGroup`.
- [ ] Update labels so `TriggerType::StepFlow.label_ja()` returns `ステップフロー`.

## Task 2: Database Migration And Seed Data

**Files:**
- Create: `crates/controller/migrations/20260605000001_step_flows_and_remove_dag.sql`
- Modify: existing seed migrations that create `dag-lambda-job`, `dag_tasks`, `dag_edges`, and `task_runs`.

- [ ] Add StepFlow tables with FK constraints and persisted `run_number`.
- [ ] Remove DAG enum value from `jobs.job_type`.
- [ ] Remove DAG-only seed data and add `step-flow` seed rows.
- [ ] Ensure StepFlow history v1 is seeded.

## Task 3: StepFlow DB Layer

**Files:**
- Create: `crates/controller/src/db/step_flows.rs`
- Modify: `crates/controller/src/db/mod.rs`
- Modify: `crates/controller/src/db/jobs.rs`

- [ ] Add failing tests for group run condition validation and run number allocation helpers.
- [ ] Implement list/get/create/update/history helpers.
- [ ] Implement job deletion guard that rejects deleting jobs referenced by StepFlow steps.

## Task 4: Job UI DAG Removal

**Files:**
- Modify: `crates/controller/src/web/jobs.rs`
- Modify: `crates/controller/templates/jobs/form.html`
- Modify: `crates/controller/templates/jobs/list.html`
- Modify: `crates/controller/templates/jobs/detail.html`
- Modify: `crates/controller/templates/runs/detail.html`
- Modify: `crates/controller/templates/runs/detail_live.html`
- Modify: `static/css/style.css`
- Modify: `docs/ui_checklists/job_edit.md`
- Modify: `docs/ui_checklists/job_list.md`
- Modify: `docs/ui_checklists/job_detail.md`
- Modify: `docs/ui_checklists/run_detail.md`

- [ ] Remove DAG job type choices and DAG JSON form sections.
- [ ] Remove DAG graph visualization from job and run details.
- [ ] Update labels to `Cron / OneShot`.

## Task 5: StepFlow Web UI

**Files:**
- Create: `crates/controller/src/web/step_flows.rs`
- Modify: `crates/controller/src/web/mod.rs`
- Modify: `crates/controller/templates/base.html`
- Create: `crates/controller/templates/step_flows/list.html`
- Create: `crates/controller/templates/step_flows/form.html`
- Create: `crates/controller/templates/step_flows/detail.html`
- Create: `crates/controller/templates/step_flows/run_detail.html`
- Create: `docs/ui_checklists/step_flow_list.md`
- Create: `docs/ui_checklists/step_flow_form.md`
- Create: `docs/ui_checklists/step_flow_detail.md`

- [ ] Add sidebar item `ジョブ管理 > ステップフロー`.
- [ ] Add list, create, edit, detail, and run detail pages.
- [ ] Group 1 hides run condition. Group 2+ shows `前Group成功時のみ` and `常に実行`.
- [ ] Job selector includes all spaces with labels like `space / job`.

## Task 6: StepFlow Execution

**Files:**
- Create: `crates/controller/src/scheduler/step_flow_engine.rs`
- Modify: `crates/controller/src/scheduler/mod.rs`
- Modify: `crates/controller/src/api/callback.rs`
- Modify: `crates/controller/src/db/runs.rs`

- [ ] Add parent StepFlow run creation with locked run numbering.
- [ ] Launch Group 1 child `job_runs` with `TriggerType::StepFlow`.
- [ ] Re-evaluate parent StepFlow runs from worker callbacks.
- [ ] Guard evaluation with DB transaction/lock to avoid multi-controller double-launch.

## Task 7: Verification

- [ ] Run `cargo fmt --check --all`.
- [ ] Run `cargo check --workspace`.
- [ ] Run `cargo test --workspace`.
- [ ] Run `cargo clippy --workspace -- -D warnings`.
- [ ] Reset local DB, run migrations, run seed data.
- [ ] Restart server and verify `/jobs/new`, `/step-flows`, `step-flow` detail, and a StepFlow run in browser.
