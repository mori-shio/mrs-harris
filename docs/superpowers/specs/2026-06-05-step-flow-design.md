# StepFlow Design

## Goal

Replace the current DAG job type with a separate "ステップフロー" concept.

StepFlow is an orchestration feature that runs existing jobs in ordered groups. Jobs inside the same group run in parallel. Groups run sequentially from top to bottom.

This design intentionally separates StepFlow from `JobType`. A job remains a worker-backed unit of execution. A StepFlow is a controller-side orchestration definition that references jobs.

## User Model

The sidebar under job management becomes:

```text
ジョブ管理
  - ジョブ
  - ステップフロー
  - スペース
```

Jobs keep only the executable job types:

```text
OneShot
Cron
```

`DAG` is removed from job type choices and from user-facing labels.

StepFlow is managed on its own screens:

```text
/step-flows
/step-flows/new
/step-flows/{name}
/step-flows/{name}/edit
/step-flows/{name}/runs/{run_number}
```

## StepFlow Structure

A StepFlow contains one or more groups.

Each group contains one or more steps.

Each step references an existing job.

Example:

```text
ステップフロー: daily-processing

Group 1
  - ingest-orders
  - ingest-users

Group 2                 run_condition = on_success
  - normalize-data
  - update-index

Group 3                 run_condition = always
  - notify-result
```

Execution rules:

- Steps in the same group run in parallel.
- Groups run sequentially by `group_order`.
- Group 1 has no previous group, so it does not have `run_condition`.
- Group 2 and later groups require `run_condition`.
- `on_success` runs only when the previous group succeeded.
- `always` runs when the previous group succeeded, failed, or was skipped.
- A group succeeds when all launched child job runs succeed.
- A group fails when at least one launched child job run fails.
- A group that does not match its `run_condition` is skipped and evaluation continues to the next group.
- A group with no launchable steps after condition evaluation is skipped and evaluation continues to the next group.
- The StepFlow run succeeds only when evaluation reaches the end without any failed group.
- The StepFlow run fails if any group failed, even if later `always` steps also ran.

## Referenced Jobs

StepFlow can reference any job from any space.

Allowed referenced job types after DAG removal:

- `OneShot`
- `Cron`

Cron jobs are treated as immediate child executions when launched by StepFlow. Their `schedule_expr` is not used for StepFlow child runs.

StepFlow does not reference another StepFlow in the initial design. That avoids recursive orchestration and keeps cycle detection scoped to job references.

The job selector should show enough context to disambiguate jobs across spaces:

```text
default / test-job
default / cron-fargate-job
ops / nightly-backup
ml / train-model
```

If a job is referenced by any StepFlow, deleting that job should be rejected with a clear message.

## Database Model

Use snake_case table names with `step_flow` as separate words:

```text
step_flows
step_flow_groups
step_flow_steps
step_flow_runs
step_flow_step_runs
step_flow_history
```

### `step_flows`

Definition shown in the StepFlow list.

Fields:

```text
id
name
description
space_id
is_active
timeout_sec
tags
created_at
updated_at
```

### `step_flow_groups`

Ordered groups inside a StepFlow.

Fields:

```text
id
step_flow_id
group_order
run_condition NULL | on_success | always
created_at
updated_at
```

Validation:

- If `group_order = 1`, `run_condition` must be `NULL`.
- If `group_order > 1`, `run_condition` must be `on_success` or `always`.

Implement this validation in Rust when saving StepFlow definitions. MySQL `CHECK` constraints can be added later if the deployment target supports them consistently.

### `step_flow_steps`

One job reference inside a group.

Fields:

```text
id
group_id
step_order
job_id
created_at
updated_at
```

### `step_flow_runs`

One execution history record for a StepFlow.

Fields:

```text
id
step_flow_id
step_flow_history_id
run_number
status
trigger_type
created_by
created_at
started_at
finished_at
duration_ms
```

Rules:

- `run_number` is persisted, not computed with `COUNT(*)`.
- Numbering must be allocated inside a transaction with locking.
- `step_flow_history_id` is required.

### `step_flow_step_runs`

Mapping between a StepFlow step and the child `job_runs` record it launched.

Fields:

```text
id
step_flow_run_id
step_flow_step_id
job_id
job_history_id
job_run_id
status
started_at
finished_at
created_at
updated_at
```

Rules:

- `job_history_id` is required.
- `job_run_id` is set when the child job run is created.
- The child `job_runs.job_history_id` must match this `job_history_id`.

### `step_flow_history`

Versioned StepFlow definition snapshots.

Fields:

```text
id
step_flow_id
version
payload
changed_by
changed_at
```

The payload stores the StepFlow definition at that version, including groups, steps, referenced job names/IDs, and run conditions.

## History Pinning

StepFlow execution must be reproducible.

When creating `step_flow_runs`:

1. Ensure the latest StepFlow history exists.
2. Store that history ID in `step_flow_runs.step_flow_history_id`.
3. When launching each child job, resolve and store the referenced job's current `job_history_id`.
4. Store that `job_history_id` in both `step_flow_step_runs.job_history_id` and the child `job_runs.job_history_id`.

This prevents edits to either the StepFlow or referenced jobs from changing an already-started execution.

## Execution Engine

Add a StepFlow scheduler/evaluator separate from the existing job scheduler.

Flow:

1. User starts a StepFlow.
2. Controller creates `step_flow_runs`.
3. Controller evaluates Group 1.
4. Controller creates child `job_runs` for all launchable steps in Group 1.
5. Worker callbacks update child `job_runs`.
6. Callback handling asks the StepFlow evaluator to re-check affected parent runs.
7. Evaluator determines whether the current group is complete.
8. If complete, evaluator starts the next group.
9. When all groups are complete, evaluator marks the parent StepFlow run as `succeeded` or `failed`.

Multi-controller safety:

- Evaluation for a given `step_flow_run_id` must be guarded by a DB lock, lease, or claim.
- The same next group must not be launched twice when multiple controller replicas handle callbacks at the same time.
- Child `job_runs` creation for StepFlow steps must be idempotent per `(step_flow_run_id, step_flow_step_id)`.

Trigger type:

- Add `TriggerType::StepFlow`.
- Child job runs created by StepFlow use `TriggerType::StepFlow`.
- Existing `TriggerType::Dependency` should either be removed if only DAG used it, or relabeled away from `DAG依存`.

## DAG Removal

Remove DAG from the application model:

- Remove `JobType::Dag`.
- Remove `dag_tasks`.
- Remove `dag_edges`.
- Remove `task_runs` if it is only used for DAG tasks.
- Remove or replace `dag_engine`.
- Remove DAG form sections.
- Remove DAG graph visualization.
- Remove DAG seed data.
- Replace user-facing `DAG` labels in templates, docs checklists, tests, and seed snapshots.

No DAG compatibility migration is required because existing production/local DBs are assumed to have no DAG jobs.

Use new migrations that:

- Change `jobs.job_type` enum from `cron | dag | one_shot` to `cron | one_shot`.
- Add StepFlow tables.
- Drop DAG-only tables after any local seed reset assumptions are satisfied.

## Seed Data

Replace the existing `dag-lambda-job` seed with StepFlow seed data.

Use the name:

```text
step-flow
```

Do not use a worker-specific name such as `step-lambda-job`, because StepFlow is orchestration and does not itself run on Lambda/Fargate.

Seed should include:

- At least one StepFlow definition.
- At least two groups.
- At least one group with parallel steps.
- At least one Group 2+ group with `on_success`.
- At least one Group 2+ group with `always`.
- StepFlow history v1.
- Optional sample StepFlow run and StepFlow step run records if needed for UI development.

After implementation, local verification must reset the DB, run migrations, and seed from scratch so no DAG data remains.

## UI Requirements

Before implementation, add UI checklist items under `docs/ui_checklists/` for:

- Job type choices no longer show DAG.
- Sidebar shows `ジョブ管理 > ステップフロー`.
- StepFlow list shows StepFlow definitions.
- StepFlow create/edit supports groups and steps.
- Group 1 does not show a run condition selector.
- Group 2+ headers show `前Group成功時のみ` and `常に実行`.
- Job selector includes jobs from all spaces and displays space context.
- StepFlow detail shows groups, steps, referenced jobs, and latest version.
- StepFlow run detail shows parent run status and child job run links.
- StepFlow history modal shows versioned readonly details.

Initial UI can use structured form controls rather than JSON. The point of StepFlow is to replace DAG's JSON-heavy experience with an understandable group/step editor.

## Testing And Verification

Rust verification:

```bash
cargo fmt --check --all
cargo check --workspace
cargo test --workspace
cargo clippy --workspace -- -D warnings
```

DB verification:

```text
reset local DB
run all migrations
run seed data
confirm jobs.job_type has no dag values
confirm step-flow seed exists
confirm step_flow_history is populated
confirm StepFlow run numbering is persisted
confirm child job_runs have TriggerType::StepFlow and pinned job_history_id
```

Browser verification:

- Open `/jobs/new` and confirm DAG is gone.
- Open `/step-flows`.
- Create or inspect `step-flow`.
- Confirm all-space job selection labels include space names.
- Confirm Group 1 has no run condition controls.
- Confirm Group 2+ has run condition controls.
- Start StepFlow and inspect `/step-flows/{name}/runs/{run_number}`.
- Confirm child job run links open existing run detail pages.

## Open Decisions For Implementation

These should be decided while writing the implementation plan:

- Whether `step_flow_runs` should support cancellation in the first release.
- Whether running child job runs should be cancelled when the parent StepFlow run is cancelled.
- Whether StepFlow runs appear on the dashboard immediately, or only on StepFlow detail pages in the first release.
- Whether StepFlow notifications reuse job notification settings or get their own settings later.
