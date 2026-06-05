use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, State},
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use mrs_harris_common::models::step_flow::{
    NewStepFlow, StepFlow, StepFlowGroup, StepFlowRun, StepFlowRunCondition, StepFlowStep,
};
use sqlx::Row;

use super::auth::WebClaims;
use crate::app::AppState;

#[derive(Clone)]
struct JobOption {
    id: i64,
    label: String,
}

#[derive(Clone)]
struct StepFlowListItem {
    name: String,
    description: String,
    is_active: bool,
    tags: Vec<String>,
}

#[derive(Clone)]
struct StepFlowStepView {
    order: u32,
    job_label: String,
}

#[derive(Clone)]
struct StepFlowGroupView {
    order: u32,
    condition_label: String,
    steps: Vec<StepFlowStepView>,
}

#[derive(Template)]
#[template(path = "step_flows/list.html")]
struct StepFlowListTemplate {
    flows: Vec<StepFlowListItem>,
}
crate::impl_into_response!(StepFlowListTemplate);

#[derive(Template)]
#[template(path = "step_flows/form.html")]
struct StepFlowFormTemplate {
    jobs: Vec<JobOption>,
    spaces: Vec<mrs_harris_common::models::space::Space>,
    error_message: Option<String>,
}
crate::impl_into_response!(StepFlowFormTemplate);

#[derive(Template)]
#[template(path = "step_flows/detail.html")]
struct StepFlowDetailTemplate {
    flow: StepFlow,
    groups: Vec<StepFlowGroupView>,
    latest_version: u32,
}
crate::impl_into_response!(StepFlowDetailTemplate);

#[derive(Template)]
#[template(path = "step_flows/run_detail.html")]
struct StepFlowRunDetailTemplate {
    flow: StepFlow,
    run: StepFlowRun,
}
crate::impl_into_response!(StepFlowRunDetailTemplate);

#[derive(serde::Deserialize)]
struct StepFlowForm {
    name: String,
    description: Option<String>,
    space_id: Option<i64>,
    tags: Option<String>,
    group1_jobs: Vec<i64>,
    group2_condition: Option<String>,
    group2_jobs: Option<Vec<i64>>,
    group3_condition: Option<String>,
    group3_jobs: Option<Vec<i64>>,
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/step-flows", get(list_page))
        .route("/step-flows/new", get(new_page).post(create_submit))
        .route("/step-flows/{name}", get(detail_page))
        .route("/step-flows/{name}/run", post(run_submit))
        .route("/step-flows/{name}/runs/{run_number}", get(run_detail_page))
}

async fn list_page(State(state): State<AppState>, _claims: WebClaims) -> impl IntoResponse {
    let flows = crate::db::step_flows::list_step_flows(&state.db)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|flow| StepFlowListItem {
            name: flow.name,
            description: flow.description.unwrap_or_default(),
            is_active: flow.is_active,
            tags: flow.tags,
        })
        .collect();

    StepFlowListTemplate { flows }
}

async fn new_page(State(state): State<AppState>, _claims: WebClaims) -> impl IntoResponse {
    StepFlowFormTemplate {
        jobs: load_job_options(&state).await,
        spaces: load_spaces(&state).await,
        error_message: None,
    }
}

async fn create_submit(
    State(state): State<AppState>,
    claims: WebClaims,
    Form(form): Form<StepFlowForm>,
) -> impl IntoResponse {
    let groups = match build_groups_from_form(&form) {
        Ok(groups) => groups,
        Err(message) => {
            return StepFlowFormTemplate {
                jobs: load_job_options(&state).await,
                spaces: load_spaces(&state).await,
                error_message: Some(message),
            }
            .into_response();
        }
    };

    let tags = form
        .tags
        .unwrap_or_default()
        .split(',')
        .map(|tag| tag.trim().to_string())
        .filter(|tag| !tag.is_empty())
        .collect();

    let new_flow = NewStepFlow {
        name: form.name.trim().to_string(),
        description: form.description.filter(|value| !value.trim().is_empty()),
        space_id: form.space_id,
        is_active: true,
        timeout_sec: 3600,
        tags,
        groups,
    };

    match crate::db::step_flows::create_step_flow(&state.db, &new_flow, &claims.0.username).await {
        Ok(flow) => Redirect::to(&format!("/step-flows/{}", flow.name)).into_response(),
        Err(err) => StepFlowFormTemplate {
            jobs: load_job_options(&state).await,
            spaces: load_spaces(&state).await,
            error_message: Some(format!("ステップフローを保存できませんでした: {}", err)),
        }
        .into_response(),
    }
}

async fn detail_page(
    State(state): State<AppState>,
    _claims: WebClaims,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(flow) = crate::db::step_flows::get_step_flow_by_name(&state.db, &name)
        .await
        .ok()
        .flatten()
    else {
        return Redirect::to("/step-flows").into_response();
    };

    let groups = build_group_views(&state, flow.id).await;
    let latest_version = crate::db::step_flows::latest_history(&state.db, flow.id)
        .await
        .ok()
        .flatten()
        .map(|history| history.version)
        .unwrap_or(1);

    StepFlowDetailTemplate {
        flow,
        groups,
        latest_version,
    }
    .into_response()
}

async fn run_submit(
    State(state): State<AppState>,
    claims: WebClaims,
    Path(name): Path<String>,
) -> impl IntoResponse {
    let Some(flow) = crate::db::step_flows::get_step_flow_by_name(&state.db, &name)
        .await
        .ok()
        .flatten()
    else {
        return Redirect::to("/step-flows").into_response();
    };

    match start_step_flow(&state, flow.id, &claims.0.username).await {
        Ok(run) => Redirect::to(&format!("/step-flows/{}/runs/{}", name, run.run_number)),
        Err(err) => {
            tracing::error!("Failed to start StepFlow {}: {}", flow.name, err);
            Redirect::to(&format!("/step-flows/{}", name))
        }
    }
    .into_response()
}

async fn run_detail_page(
    State(state): State<AppState>,
    _claims: WebClaims,
    Path((name, run_number)): Path<(String, i64)>,
) -> impl IntoResponse {
    let Some(flow) = crate::db::step_flows::get_step_flow_by_name(&state.db, &name)
        .await
        .ok()
        .flatten()
    else {
        return Redirect::to("/step-flows").into_response();
    };

    let Some(run) =
        crate::db::step_flows::get_step_flow_run_by_number(&state.db, flow.id, run_number)
            .await
            .ok()
            .flatten()
    else {
        return Redirect::to(&format!("/step-flows/{}", name)).into_response();
    };

    StepFlowRunDetailTemplate { flow, run }.into_response()
}

fn build_groups_from_form(form: &StepFlowForm) -> Result<Vec<StepFlowGroup>, String> {
    if form.group1_jobs.is_empty() {
        return Err("Group 1 には少なくとも1つのジョブを選択してください。".to_string());
    }

    let mut groups = vec![StepFlowGroup {
        id: None,
        step_flow_id: None,
        group_order: 1,
        run_condition: None,
        steps: build_steps(&form.group1_jobs),
    }];

    if let Some(jobs) = &form.group2_jobs
        && !jobs.is_empty()
    {
        groups.push(StepFlowGroup {
            id: None,
            step_flow_id: None,
            group_order: 2,
            run_condition: Some(parse_condition(form.group2_condition.as_deref())?),
            steps: build_steps(jobs),
        });
    }

    if let Some(jobs) = &form.group3_jobs
        && !jobs.is_empty()
    {
        groups.push(StepFlowGroup {
            id: None,
            step_flow_id: None,
            group_order: 3,
            run_condition: Some(parse_condition(form.group3_condition.as_deref())?),
            steps: build_steps(jobs),
        });
    }

    Ok(groups)
}

fn build_steps(job_ids: &[i64]) -> Vec<StepFlowStep> {
    job_ids
        .iter()
        .enumerate()
        .map(|(index, job_id)| StepFlowStep {
            id: None,
            group_id: None,
            step_order: index as u32 + 1,
            job_id: *job_id,
        })
        .collect()
}

fn parse_condition(value: Option<&str>) -> Result<StepFlowRunCondition, String> {
    match value {
        Some("always") => Ok(StepFlowRunCondition::Always),
        Some("on_success") | None => Ok(StepFlowRunCondition::OnSuccess),
        Some(other) => Err(format!("不正な実行条件です: {}", other)),
    }
}

async fn load_job_options(state: &AppState) -> Vec<JobOption> {
    let rows = sqlx::query(
        "SELECT j.id, j.name, COALESCE(s.name, '未分類') AS space_name \
         FROM jobs j LEFT JOIN spaces s ON j.space_id = s.id ORDER BY s.priority ASC, s.name ASC, j.name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|row| {
            let id: i64 = row.try_get("id").ok()?;
            let name: String = row.try_get("name").ok()?;
            let space_name: String = row.try_get("space_name").ok()?;
            Some(JobOption {
                id,
                label: format!("{} / {}", space_name, name),
            })
        })
        .collect()
}

async fn load_spaces(state: &AppState) -> Vec<mrs_harris_common::models::space::Space> {
    let rows = sqlx::query(
        "SELECT id, name, description, priority, created_at, updated_at FROM spaces ORDER BY priority ASC, name ASC",
    )
    .fetch_all(&state.db)
    .await
    .unwrap_or_default();

    rows.into_iter()
        .filter_map(|row| {
            Some(mrs_harris_common::models::space::Space {
                id: row.try_get("id").ok()?,
                name: row.try_get("name").ok()?,
                description: row.try_get("description").ok(),
                priority: row.try_get("priority").unwrap_or_default(),
                created_at: row.try_get("created_at").ok()?,
                updated_at: row.try_get("updated_at").ok()?,
            })
        })
        .collect()
}

async fn build_group_views(state: &AppState, step_flow_id: i64) -> Vec<StepFlowGroupView> {
    let groups = crate::db::step_flows::list_groups(&state.db, step_flow_id)
        .await
        .unwrap_or_default();
    let jobs = load_job_options(state).await;

    groups
        .into_iter()
        .map(|group| StepFlowGroupView {
            order: group.group_order,
            condition_label: match group.run_condition {
                None => "条件なし".to_string(),
                Some(StepFlowRunCondition::OnSuccess) => "前Group成功時のみ".to_string(),
                Some(StepFlowRunCondition::Always) => "常に実行".to_string(),
            },
            steps: group
                .steps
                .into_iter()
                .map(|step| StepFlowStepView {
                    order: step.step_order,
                    job_label: jobs
                        .iter()
                        .find(|job| job.id == step.job_id)
                        .map(|job| job.label.clone())
                        .unwrap_or_else(|| format!("job_id={}", step.job_id)),
                })
                .collect(),
        })
        .collect()
}

async fn start_step_flow(
    state: &AppState,
    step_flow_id: i64,
    username: &str,
) -> anyhow::Result<StepFlowRun> {
    let run =
        crate::db::step_flows::create_step_flow_run(&state.db, step_flow_id, username).await?;
    crate::scheduler::step_flow_engine::evaluate_run(state, run.id).await?;
    Ok(run)
}
