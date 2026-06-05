use mrs_harris_common::models::run::{NewRun, RunStatus, TriggerType};
use mrs_harris_common::models::step_flow::{StepFlowGroup, StepFlowRunCondition};
use sqlx::{MySql, Row, Transaction};
use std::str::FromStr;

use crate::app::AppState;

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
enum GroupOutcome {
    Succeeded,
    Failed,
    Skipped,
}

pub async fn handle_child_run_update(state: &AppState, job_run_id: i64) -> anyhow::Result<()> {
    let parent_run_id: Option<i64> =
        sqlx::query_scalar("SELECT step_flow_run_id FROM step_flow_step_runs WHERE job_run_id = ?")
            .bind(job_run_id)
            .fetch_optional(&state.db)
            .await?;

    let Some(parent_run_id) = parent_run_id else {
        return Ok(());
    };

    sqlx::query(
        "UPDATE step_flow_step_runs sfr \
         JOIN job_runs jr ON jr.id = sfr.job_run_id \
         SET sfr.status = jr.status, sfr.finished_at = jr.finished_at \
         WHERE sfr.job_run_id = ?",
    )
    .bind(job_run_id)
    .execute(&state.db)
    .await?;

    evaluate_run(state, parent_run_id).await
}

pub async fn evaluate_run(state: &AppState, step_flow_run_id: i64) -> anyhow::Result<()> {
    let mut tx = state.db.begin().await?;

    let run_row =
        sqlx::query("SELECT id, step_flow_id, status FROM step_flow_runs WHERE id = ? FOR UPDATE")
            .bind(step_flow_run_id)
            .fetch_optional(&mut *tx)
            .await?;

    let Some(run_row) = run_row else {
        tx.commit().await?;
        return Ok(());
    };

    let status = RunStatus::from_str(run_row.try_get::<String, _>("status")?.as_str())
        .map_err(|e| anyhow::anyhow!("Invalid StepFlow run status: {}", e))?;
    if status.is_terminal() {
        tx.commit().await?;
        return Ok(());
    }

    let step_flow_id: i64 = run_row.try_get("step_flow_id")?;
    let groups = crate::db::step_flows::list_groups(&state.db, step_flow_id).await?;

    let mut previous = GroupOutcome::Succeeded;
    let mut has_failed_group = false;

    for group in groups {
        if !should_run_group(&group, previous) {
            previous = GroupOutcome::Skipped;
            continue;
        }

        let statuses = load_group_statuses(&mut tx, step_flow_run_id, group.id).await?;
        if statuses.iter().any(Option::is_none) {
            tx.commit().await?;
            launch_group(state, step_flow_run_id, &group).await?;
            return Ok(());
        }

        let statuses: Vec<RunStatus> = statuses.into_iter().flatten().collect();
        if statuses.iter().any(|status| !status.is_terminal()) {
            tx.commit().await?;
            return Ok(());
        }

        if statuses.iter().any(|status| {
            matches!(
                status,
                RunStatus::Failed | RunStatus::Cancelled | RunStatus::DeadLetter
            )
        }) {
            has_failed_group = true;
            previous = GroupOutcome::Failed;
        } else {
            previous = GroupOutcome::Succeeded;
        }
    }

    let final_status = if has_failed_group {
        RunStatus::Failed
    } else {
        RunStatus::Succeeded
    };
    sqlx::query(
        "UPDATE step_flow_runs \
         SET status = ?, finished_at = UTC_TIMESTAMP(), duration_ms = TIMESTAMPDIFF(MICROSECOND, started_at, UTC_TIMESTAMP()) DIV 1000 \
         WHERE id = ?",
    )
    .bind(final_status.to_string())
    .bind(step_flow_run_id)
    .execute(&mut *tx)
    .await?;

    tx.commit().await?;
    Ok(())
}

fn should_run_group(group: &StepFlowGroup, previous: GroupOutcome) -> bool {
    match group.run_condition {
        None => true,
        Some(StepFlowRunCondition::OnSuccess) => previous == GroupOutcome::Succeeded,
        Some(StepFlowRunCondition::Always) => true,
    }
}

async fn load_group_statuses(
    tx: &mut Transaction<'_, MySql>,
    step_flow_run_id: i64,
    group_id: Option<i64>,
) -> anyhow::Result<Vec<Option<RunStatus>>> {
    let group_id = group_id.ok_or_else(|| anyhow::anyhow!("StepFlow group has no id"))?;
    let rows = sqlx::query(
        "SELECT jr.status \
         FROM step_flow_steps sfs \
         LEFT JOIN step_flow_step_runs sfr ON sfr.step_flow_step_id = sfs.id AND sfr.step_flow_run_id = ? \
         LEFT JOIN job_runs jr ON jr.id = sfr.job_run_id \
         WHERE sfs.group_id = ? \
         ORDER BY sfs.step_order ASC",
    )
    .bind(step_flow_run_id)
    .bind(group_id)
    .fetch_all(&mut **tx)
    .await?;

    rows.into_iter()
        .map(|row| {
            let status: Option<String> = row.try_get("status")?;
            status
                .map(|value| {
                    RunStatus::from_str(&value)
                        .map_err(|e| anyhow::anyhow!("Invalid child run status: {}", e))
                })
                .transpose()
        })
        .collect()
}

async fn launch_group(
    state: &AppState,
    step_flow_run_id: i64,
    group: &StepFlowGroup,
) -> anyhow::Result<()> {
    for step in &group.steps {
        let step_id = step
            .id
            .ok_or_else(|| anyhow::anyhow!("StepFlow step has no id"))?;
        let existing: Option<i64> = sqlx::query_scalar(
            "SELECT job_run_id FROM step_flow_step_runs WHERE step_flow_run_id = ? AND step_flow_step_id = ?",
        )
        .bind(step_flow_run_id)
        .bind(step_id)
        .fetch_optional(&state.db)
        .await?;
        if existing.is_some() {
            continue;
        }

        let job = crate::db::jobs::get_job(&state.db, &step.job_id)
            .await?
            .ok_or_else(|| anyhow::anyhow!("Referenced job {} not found", step.job_id))?;
        let child_run = crate::db::runs::create_run(
            &state.db,
            &NewRun {
                job_id: job.id,
                worker_type: job.worker_type.clone(),
                trigger_type: TriggerType::StepFlow,
                scheduled_at: None,
                worker_definition_id: job.worker_definition_id,
                worker_definition_history_id: None,
            },
        )
        .await?;
        let job_history_id = child_run
            .job_history_id
            .ok_or_else(|| anyhow::anyhow!("Child run has no job_history_id"))?;

        sqlx::query(
            "INSERT IGNORE INTO step_flow_step_runs \
             (step_flow_run_id, step_flow_step_id, job_id, job_history_id, job_run_id, status) \
             VALUES (?, ?, ?, ?, ?, ?)",
        )
        .bind(step_flow_run_id)
        .bind(step_id)
        .bind(job.id)
        .bind(job_history_id)
        .bind(child_run.id)
        .bind(child_run.status.to_string())
        .execute(&state.db)
        .await?;
    }

    Ok(())
}
