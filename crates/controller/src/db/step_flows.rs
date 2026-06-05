use chrono::Utc;
use mrs_harris_common::models::run::{RunStatus, TriggerType};
use mrs_harris_common::models::step_flow::{
    NewStepFlow, StepFlow, StepFlowGroup, StepFlowHistoryEntry, StepFlowRun, StepFlowRunCondition,
    StepFlowStep,
};
use sqlx::{MySqlPool, Row};
use std::str::FromStr;

fn map_step_flow(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<StepFlow> {
    let tags_val: serde_json::Value = row.try_get("tags")?;
    let is_active_val: i8 = row.try_get("is_active")?;

    Ok(StepFlow {
        id: row.try_get("id")?,
        name: row.try_get("name")?,
        description: row.try_get("description")?,
        space_id: row.try_get("space_id")?,
        is_active: is_active_val != 0,
        timeout_sec: row.try_get("timeout_sec")?,
        tags: serde_json::from_value(tags_val)?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

fn map_step_flow_run(row: &sqlx::mysql::MySqlRow) -> anyhow::Result<StepFlowRun> {
    let status_str: String = row.try_get("status")?;
    let trigger_type_str: String = row.try_get("trigger_type")?;

    Ok(StepFlowRun {
        id: row.try_get("id")?,
        step_flow_id: row.try_get("step_flow_id")?,
        step_flow_history_id: row.try_get("step_flow_history_id")?,
        run_number: row.try_get("run_number")?,
        status: RunStatus::from_str(&status_str)
            .map_err(|e| anyhow::anyhow!("Invalid StepFlow run status: {}", e))?,
        trigger_type: TriggerType::from_str(&trigger_type_str)
            .map_err(|e| anyhow::anyhow!("Invalid StepFlow trigger type: {}", e))?,
        created_by: row.try_get("created_by")?,
        started_at: row.try_get("started_at")?,
        finished_at: row.try_get("finished_at")?,
        duration_ms: row.try_get("duration_ms")?,
        created_at: row.try_get("created_at")?,
        updated_at: row.try_get("updated_at")?,
    })
}

pub async fn list_step_flows(pool: &MySqlPool) -> anyhow::Result<Vec<StepFlow>> {
    let rows = sqlx::query("SELECT * FROM step_flows ORDER BY created_at DESC")
        .fetch_all(pool)
        .await?;
    rows.iter().map(map_step_flow).collect()
}

pub async fn get_step_flow_by_name(
    pool: &MySqlPool,
    name: &str,
) -> anyhow::Result<Option<StepFlow>> {
    let row = sqlx::query("SELECT * FROM step_flows WHERE name = ?")
        .bind(name)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_step_flow).transpose()
}

pub async fn list_groups(
    pool: &MySqlPool,
    step_flow_id: i64,
) -> anyhow::Result<Vec<StepFlowGroup>> {
    let rows = sqlx::query(
        "SELECT id, step_flow_id, group_order, run_condition FROM step_flow_groups WHERE step_flow_id = ? ORDER BY group_order ASC",
    )
    .bind(step_flow_id)
    .fetch_all(pool)
    .await?;

    let mut groups = Vec::new();
    for row in rows {
        let group_id: i64 = row.try_get("id")?;
        let condition_str: Option<String> = row.try_get("run_condition")?;
        groups.push(StepFlowGroup {
            id: Some(group_id),
            step_flow_id: Some(row.try_get("step_flow_id")?),
            group_order: row.try_get("group_order")?,
            run_condition: condition_str
                .map(|value| StepFlowRunCondition::from_str(&value))
                .transpose()
                .map_err(|e| anyhow::anyhow!("Invalid StepFlow run_condition: {}", e))?,
            steps: list_steps(pool, group_id).await?,
        });
    }
    Ok(groups)
}

pub async fn list_steps(pool: &MySqlPool, group_id: i64) -> anyhow::Result<Vec<StepFlowStep>> {
    let rows = sqlx::query(
        "SELECT id, group_id, step_order, job_id FROM step_flow_steps WHERE group_id = ? ORDER BY step_order ASC",
    )
    .bind(group_id)
    .fetch_all(pool)
    .await?;

    rows.iter()
        .map(|row| {
            Ok(StepFlowStep {
                id: Some(row.try_get("id")?),
                group_id: Some(row.try_get("group_id")?),
                step_order: row.try_get("step_order")?,
                job_id: row.try_get("job_id")?,
            })
        })
        .collect()
}

pub async fn create_step_flow(
    pool: &MySqlPool,
    new_flow: &NewStepFlow,
    changed_by: &str,
) -> anyhow::Result<StepFlow> {
    for group in &new_flow.groups {
        group
            .validate_run_condition()
            .map_err(|e| anyhow::anyhow!(e))?;
    }

    let mut tx = pool.begin().await?;
    let result = sqlx::query(
        "INSERT INTO step_flows (name, description, space_id, is_active, timeout_sec, tags) VALUES (?, ?, ?, ?, ?, ?)",
    )
    .bind(&new_flow.name)
    .bind(&new_flow.description)
    .bind(new_flow.space_id)
    .bind(if new_flow.is_active { 1i8 } else { 0i8 })
    .bind(new_flow.timeout_sec)
    .bind(serde_json::to_value(&new_flow.tags)?)
    .execute(&mut *tx)
    .await?;

    let step_flow_id = result.last_insert_id() as i64;
    for group in &new_flow.groups {
        let group_result = sqlx::query(
            "INSERT INTO step_flow_groups (step_flow_id, group_order, run_condition) VALUES (?, ?, ?)",
        )
        .bind(step_flow_id)
        .bind(group.group_order)
        .bind(group.run_condition.as_ref().map(|condition| condition.to_string()))
        .execute(&mut *tx)
        .await?;
        let group_id = group_result.last_insert_id() as i64;

        for step in &group.steps {
            sqlx::query(
                "INSERT INTO step_flow_steps (group_id, step_order, job_id) VALUES (?, ?, ?)",
            )
            .bind(group_id)
            .bind(step.step_order)
            .bind(step.job_id)
            .execute(&mut *tx)
            .await?;
        }
    }

    sqlx::query(
        "INSERT INTO step_flow_history (step_flow_id, version, payload, changed_by) VALUES (?, 1, ?, ?)",
    )
    .bind(step_flow_id)
    .bind(serde_json::to_value(new_flow)?)
    .bind(changed_by)
    .execute(&mut *tx)
    .await?;
    tx.commit().await?;

    get_step_flow_by_name(pool, &new_flow.name)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created StepFlow not found"))
}

pub async fn latest_history(
    pool: &MySqlPool,
    step_flow_id: i64,
) -> anyhow::Result<Option<StepFlowHistoryEntry>> {
    let row = sqlx::query(
        "SELECT * FROM step_flow_history WHERE step_flow_id = ? ORDER BY version DESC LIMIT 1",
    )
    .bind(step_flow_id)
    .fetch_optional(pool)
    .await?;

    row.map(|row| {
        Ok(StepFlowHistoryEntry {
            id: row.try_get("id")?,
            step_flow_id: row.try_get("step_flow_id")?,
            version: row.try_get("version")?,
            payload: row.try_get("payload")?,
            changed_by: row.try_get("changed_by")?,
            changed_at: row.try_get("changed_at")?,
        })
    })
    .transpose()
}

pub async fn create_step_flow_run(
    pool: &MySqlPool,
    step_flow_id: i64,
    created_by: &str,
) -> anyhow::Result<StepFlowRun> {
    let mut tx = pool.begin().await?;
    let history_id: Option<i64> =
        sqlx::query_scalar("SELECT MAX(id) FROM step_flow_history WHERE step_flow_id = ?")
            .bind(step_flow_id)
            .fetch_one(&mut *tx)
            .await?;
    let history_id =
        history_id.ok_or_else(|| anyhow::anyhow!("step_flow {} has no history", step_flow_id))?;

    let max_run_number: i64 = sqlx::query_scalar(
        "SELECT COALESCE(MAX(run_number), 0) FROM step_flow_runs WHERE step_flow_id = ? FOR UPDATE",
    )
    .bind(step_flow_id)
    .fetch_one(&mut *tx)
    .await?;

    let result = sqlx::query(
        "INSERT INTO step_flow_runs (step_flow_id, step_flow_history_id, run_number, status, trigger_type, created_by, started_at) VALUES (?, ?, ?, 'running', 'manual', ?, ?)",
    )
    .bind(step_flow_id)
    .bind(history_id)
    .bind(max_run_number + 1)
    .bind(created_by)
    .bind(Utc::now())
    .execute(&mut *tx)
    .await?;
    let run_id = result.last_insert_id() as i64;
    tx.commit().await?;

    get_step_flow_run(pool, run_id)
        .await?
        .ok_or_else(|| anyhow::anyhow!("Created StepFlow run not found"))
}

pub async fn get_step_flow_run(pool: &MySqlPool, id: i64) -> anyhow::Result<Option<StepFlowRun>> {
    let row = sqlx::query("SELECT * FROM step_flow_runs WHERE id = ?")
        .bind(id)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_step_flow_run).transpose()
}

pub async fn get_step_flow_run_by_number(
    pool: &MySqlPool,
    step_flow_id: i64,
    run_number: i64,
) -> anyhow::Result<Option<StepFlowRun>> {
    let row = sqlx::query("SELECT * FROM step_flow_runs WHERE step_flow_id = ? AND run_number = ?")
        .bind(step_flow_id)
        .bind(run_number)
        .fetch_optional(pool)
        .await?;
    row.as_ref().map(map_step_flow_run).transpose()
}
