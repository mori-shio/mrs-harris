use askama::Template;
use axum::{
    Form, Router,
    extract::{Path, Query, State},
    http::HeaderMap,
    response::{IntoResponse, Redirect},
    routing::{get, post},
};
use mrs_harris_common::models::step_flow::{
    NewStepFlow, StepFlow, StepFlowGroup, StepFlowRun, StepFlowRunCondition, StepFlowStep,
};
use sqlx::Row;
use std::collections::HashMap;

use super::{
    BreadcrumbItem, auth::WebClaims, highlight_search_match_html, home_breadcrumb,
    linked_space_breadcrumb, space_scoped_list_url,
};
use crate::app::AppState;

#[derive(Clone)]
struct JobOption {
    id: i64,
    label: String,
    selected_group1: bool,
    selected_group2: bool,
    selected_group3: bool,
}

#[derive(Clone)]
struct StepFlowListItem {
    name: String,
    search_text: String,
    highlighted_name: String,
    description: String,
    highlighted_description: String,
    space_id: Option<i64>,
    space_name: String,
    is_active: bool,
    tags: Vec<String>,
}

#[derive(Clone)]
struct StepFlowSpaceTab {
    id: String,
    name: String,
    is_active: bool,
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
    breadcrumbs: Vec<BreadcrumbItem>,
    flows: Vec<StepFlowListItem>,
    spaces: Vec<StepFlowSpaceTab>,
    current_space: String,
    current_search: String,
    copy_candidates_json: String,
}
crate::impl_into_response!(StepFlowListTemplate);

#[derive(Template)]
#[template(path = "step_flows/list_update.html")]
struct StepFlowListUpdateTemplate {
    breadcrumbs: Vec<BreadcrumbItem>,
    flows: Vec<StepFlowListItem>,
}
crate::impl_into_response!(StepFlowListUpdateTemplate);

#[derive(Template)]
#[template(path = "step_flows/form.html")]
struct StepFlowFormTemplate {
    breadcrumbs: Vec<BreadcrumbItem>,
    jobs: Vec<JobOption>,
    spaces: Vec<mrs_harris_common::models::space::Space>,
    form: StepFlowFormValues,
    error_message: Option<String>,
}
crate::impl_into_response!(StepFlowFormTemplate);

#[derive(Template)]
#[template(path = "step_flows/detail.html")]
struct StepFlowDetailTemplate {
    breadcrumbs: Vec<BreadcrumbItem>,
    back_href: String,
    flow: StepFlow,
    groups: Vec<StepFlowGroupView>,
    latest_version: u32,
}
crate::impl_into_response!(StepFlowDetailTemplate);

#[derive(Template)]
#[template(path = "step_flows/run_detail.html")]
struct StepFlowRunDetailTemplate {
    breadcrumbs: Vec<BreadcrumbItem>,
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

#[derive(Clone)]
struct StepFlowFormValues {
    name: String,
    description: String,
    space_id: String,
    tags: String,
    group1_jobs: Vec<i64>,
    group2_condition: String,
    group2_jobs: Vec<i64>,
    group3_condition: String,
    group3_jobs: Vec<i64>,
}

impl Default for StepFlowFormValues {
    fn default() -> Self {
        Self {
            name: String::new(),
            description: String::new(),
            space_id: String::new(),
            tags: String::new(),
            group1_jobs: Vec::new(),
            group2_condition: "on_success".to_string(),
            group2_jobs: Vec::new(),
            group3_condition: "always".to_string(),
            group3_jobs: Vec::new(),
        }
    }
}

#[derive(serde::Serialize)]
struct StepFlowCopyCandidate {
    name: String,
    description: String,
}

fn step_flows_breadcrumb_item(current: bool) -> BreadcrumbItem {
    if current {
        BreadcrumbItem::current("ステップフロー", "git-branch")
    } else {
        BreadcrumbItem::link("ステップフロー", "/step-flows", "git-branch")
    }
}

fn step_flow_list_breadcrumbs(space: Option<(&str, &str)>) -> Vec<BreadcrumbItem> {
    let mut breadcrumbs = vec![home_breadcrumb(), step_flows_breadcrumb_item(true)];
    if let Some((space_name, icon)) = space {
        breadcrumbs.push(BreadcrumbItem::current(space_name, icon));
    }
    breadcrumbs
}

fn step_flow_form_breadcrumbs() -> Vec<BreadcrumbItem> {
    vec![
        home_breadcrumb(),
        step_flows_breadcrumb_item(false),
        BreadcrumbItem::current("新規作成", ""),
    ]
}

async fn step_flow_detail_breadcrumbs(
    state: &AppState,
    flow_name: &str,
    space_id: Option<i64>,
) -> Vec<BreadcrumbItem> {
    vec![
        home_breadcrumb(),
        step_flows_breadcrumb_item(false),
        linked_space_breadcrumb(&state.db, space_id, "/step-flows").await,
        BreadcrumbItem::current(flow_name, ""),
    ]
}

fn step_flow_run_breadcrumbs(flow_name: &str, run_number: i64) -> Vec<BreadcrumbItem> {
    vec![
        home_breadcrumb(),
        step_flows_breadcrumb_item(false),
        BreadcrumbItem::link(flow_name, format!("/step-flows/{}", flow_name), ""),
        BreadcrumbItem::current(format!("#{}", run_number), ""),
    ]
}

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/step-flows", get(list_page))
        .route("/step-flows/new", get(new_page).post(create_submit))
        .route("/step-flows/{name}", get(detail_page))
        .route("/step-flows/{name}/run", post(run_submit))
        .route("/step-flows/{name}/runs/{run_number}", get(run_detail_page))
}

async fn list_page(
    State(state): State<AppState>,
    _claims: WebClaims,
    headers: HeaderMap,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let current_search = query.get("search").cloned().unwrap_or_default();
    let current_is_active = query.get("is_active").cloned().unwrap_or_default();
    let current_space = query.get("space").cloned().unwrap_or_default();

    let is_active = current_is_active
        .parse::<bool>()
        .ok()
        .filter(|_| !current_is_active.is_empty());

    let filter = crate::db::step_flows::StepFlowFilter {
        search: Some(current_search.clone()).filter(|value| !value.trim().is_empty()),
        is_active,
        space_id: Some(current_space.clone()).filter(|value| !value.trim().is_empty()),
    };

    let flows = crate::db::step_flows::list_step_flows(&state.db, &filter)
        .await
        .unwrap_or_default()
        .into_iter()
        .map(|flow| step_flow_list_item_from_flow(flow, Some(&current_search)))
        .collect();

    let flows = attach_space_names(&state, flows).await;
    let breadcrumb_space = resolve_step_flow_breadcrumb_space(&state, &current_space).await;
    let copy_candidates = flows
        .iter()
        .map(|flow| StepFlowCopyCandidate {
            name: flow.name.clone(),
            description: flow.description.clone(),
        })
        .collect::<Vec<_>>();
    let copy_candidates_json =
        serde_json::to_string(&copy_candidates).unwrap_or_else(|_| "[]".to_string());
    let is_partial = headers
        .get("hx-target")
        .map(|value| value == "step-flows-list-container")
        .unwrap_or(false);

    if is_partial {
        StepFlowListUpdateTemplate {
            breadcrumbs: step_flow_list_breadcrumbs(
                breadcrumb_space
                    .as_ref()
                    .map(|space| (space.0.as_str(), space.1)),
            ),
            flows,
        }
        .into_response()
    } else {
        StepFlowListTemplate {
            breadcrumbs: step_flow_list_breadcrumbs(
                breadcrumb_space
                    .as_ref()
                    .map(|space| (space.0.as_str(), space.1)),
            ),
            flows,
            spaces: build_space_tabs(&state, &current_space).await,
            current_space,
            current_search,
            copy_candidates_json,
        }
        .into_response()
    }
}

async fn new_page(
    State(state): State<AppState>,
    _claims: WebClaims,
    Query(query): Query<HashMap<String, String>>,
) -> impl IntoResponse {
    let form = match query
        .get("copy_from")
        .filter(|name| !name.trim().is_empty())
    {
        Some(name) => build_form_values_from_copy(&state, name).await,
        None => StepFlowFormValues::default(),
    };
    StepFlowFormTemplate {
        breadcrumbs: step_flow_form_breadcrumbs(),
        jobs: load_job_options(&state, &form).await,
        spaces: load_spaces(&state).await,
        form,
        error_message: None,
    }
}

async fn create_submit(
    State(state): State<AppState>,
    claims: WebClaims,
    Form(form): Form<StepFlowForm>,
) -> impl IntoResponse {
    let form_values = StepFlowFormValues::from(&form);
    let groups = match build_groups_from_form(&form) {
        Ok(groups) => groups,
        Err(message) => {
            return StepFlowFormTemplate {
                breadcrumbs: step_flow_form_breadcrumbs(),
                jobs: load_job_options(&state, &form_values).await,
                spaces: load_spaces(&state).await,
                form: form_values,
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
            breadcrumbs: step_flow_form_breadcrumbs(),
            jobs: load_job_options(&state, &form_values).await,
            spaces: load_spaces(&state).await,
            form: form_values,
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

    let breadcrumbs = step_flow_detail_breadcrumbs(&state, &flow.name, flow.space_id).await;

    StepFlowDetailTemplate {
        breadcrumbs,
        back_href: space_scoped_list_url("/step-flows", flow.space_id),
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

    StepFlowRunDetailTemplate {
        breadcrumbs: step_flow_run_breadcrumbs(&flow.name, run.run_number),
        flow,
        run,
    }
    .into_response()
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

impl From<&StepFlowForm> for StepFlowFormValues {
    fn from(form: &StepFlowForm) -> Self {
        Self {
            name: form.name.clone(),
            description: form.description.clone().unwrap_or_default(),
            space_id: form.space_id.map(|id| id.to_string()).unwrap_or_default(),
            tags: form.tags.clone().unwrap_or_default(),
            group1_jobs: form.group1_jobs.clone(),
            group2_condition: form
                .group2_condition
                .clone()
                .unwrap_or_else(|| "on_success".to_string()),
            group2_jobs: form.group2_jobs.clone().unwrap_or_default(),
            group3_condition: form
                .group3_condition
                .clone()
                .unwrap_or_else(|| "always".to_string()),
            group3_jobs: form.group3_jobs.clone().unwrap_or_default(),
        }
    }
}

async fn build_form_values_from_copy(state: &AppState, name: &str) -> StepFlowFormValues {
    let Some(flow) = crate::db::step_flows::get_step_flow_by_name(&state.db, name)
        .await
        .ok()
        .flatten()
    else {
        return StepFlowFormValues::default();
    };

    let groups = crate::db::step_flows::list_groups(&state.db, flow.id)
        .await
        .unwrap_or_default();
    let mut values = StepFlowFormValues {
        name: String::new(),
        description: flow.description.unwrap_or_default(),
        space_id: flow.space_id.map(|id| id.to_string()).unwrap_or_default(),
        tags: flow.tags.join(", "),
        group1_jobs: Vec::new(),
        group2_condition: "on_success".to_string(),
        group2_jobs: Vec::new(),
        group3_condition: "always".to_string(),
        group3_jobs: Vec::new(),
    };

    for group in groups {
        let job_ids = group.steps.into_iter().map(|step| step.job_id).collect();
        match group.group_order {
            1 => values.group1_jobs = job_ids,
            2 => {
                values.group2_condition = group
                    .run_condition
                    .as_ref()
                    .map(|condition| condition.to_string())
                    .unwrap_or_else(|| "on_success".to_string());
                values.group2_jobs = job_ids;
            }
            3 => {
                values.group3_condition = group
                    .run_condition
                    .as_ref()
                    .map(|condition| condition.to_string())
                    .unwrap_or_else(|| "always".to_string());
                values.group3_jobs = job_ids;
            }
            _ => {}
        }
    }

    values
}

async fn load_job_options(state: &AppState, form: &StepFlowFormValues) -> Vec<JobOption> {
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
                selected_group1: form.group1_jobs.contains(&id),
                selected_group2: form.group2_jobs.contains(&id),
                selected_group3: form.group3_jobs.contains(&id),
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

fn step_flow_list_item_from_flow(flow: StepFlow, search: Option<&str>) -> StepFlowListItem {
    let name = flow.name;
    let description = flow.description.unwrap_or_default();
    StepFlowListItem {
        highlighted_name: highlight_search_match_html(&name, search),
        highlighted_description: highlight_search_match_html(&description, search),
        search_text: format!("{name} {description}"),
        name,
        description,
        space_id: flow.space_id,
        space_name: "未分類".to_string(),
        is_active: flow.is_active,
        tags: flow.tags,
    }
}

async fn attach_space_names(
    state: &AppState,
    flows: Vec<StepFlowListItem>,
) -> Vec<StepFlowListItem> {
    let rows = sqlx::query("SELECT id, name FROM spaces")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let mut space_names = std::collections::HashMap::<i64, String>::new();
    for row in rows {
        if let (Ok(id), Ok(name)) = (row.try_get("id"), row.try_get("name")) {
            space_names.insert(id, name);
        }
    }

    flows
        .into_iter()
        .map(|mut flow| {
            if let Some(space_id) = flow.space_id
                && let Some(space_name) = space_names.get(&space_id)
            {
                flow.space_name = space_name.clone();
            }
            flow
        })
        .collect()
}

async fn build_space_tabs(state: &AppState, current_space: &str) -> Vec<StepFlowSpaceTab> {
    let rows = sqlx::query("SELECT id, name FROM spaces ORDER BY priority ASC, id ASC")
        .fetch_all(&state.db)
        .await
        .unwrap_or_default();
    let mut tabs = vec![StepFlowSpaceTab {
        id: String::new(),
        name: "すべて".to_string(),
        is_active: current_space.is_empty(),
    }];

    for row in rows {
        let id: i64 = row.try_get("id").unwrap_or_default();
        let id = id.to_string();
        tabs.push(StepFlowSpaceTab {
            is_active: current_space == id,
            id,
            name: row.try_get("name").unwrap_or_default(),
        });
    }

    tabs.push(StepFlowSpaceTab {
        id: "unclassified".to_string(),
        name: "未分類".to_string(),
        is_active: current_space == "unclassified",
    });

    tabs
}

async fn resolve_step_flow_breadcrumb_space(
    state: &AppState,
    current_space: &str,
) -> Option<(String, &'static str)> {
    if current_space.is_empty() {
        return None;
    }
    if current_space == "unclassified" {
        return Some(("未分類".to_string(), "help-circle"));
    }

    let space_id = current_space.parse::<i64>().ok()?;
    let row = sqlx::query("SELECT name FROM spaces WHERE id = ?")
        .bind(space_id)
        .fetch_optional(&state.db)
        .await
        .ok()
        .flatten()?;
    let name = row.try_get("name").ok()?;
    Some((name, "folder"))
}

async fn build_group_views(state: &AppState, step_flow_id: i64) -> Vec<StepFlowGroupView> {
    let groups = crate::db::step_flows::list_groups(&state.db, step_flow_id)
        .await
        .unwrap_or_default();
    let jobs = load_job_options(state, &StepFlowFormValues::default()).await;

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
