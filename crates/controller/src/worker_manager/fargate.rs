use crate::app::AppState;
use aws_sdk_ecs::Client as EcsClient;
use aws_sdk_ecs::types::{
    AssignPublicIp, AwsVpcConfiguration, ContainerOverride, LaunchType, NetworkConfiguration,
    TaskOverride,
};
use mrs_harris_common::models::run::JobRun;

/// Fargate タスクとしてジョブを起動
pub async fn launch(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    tracing::info!(run_id = %run.id, "Fargate タスクの起動を開始");

    let mut is_local =
        state.config.fargate.cluster_arn == "local" || state.config.fargate.cluster_arn.is_empty();

    if let Some(def_id) = run.worker_definition_id
        && let Ok(Some(def)) = crate::db::workers::get_worker_definition(&state.db, &def_id).await
        && let Some(val) = def.config.get("cluster_arn").and_then(|v| v.as_str())
    {
        is_local = val == "local" || val.is_empty();
    }

    if !is_local {
        match launch_aws_fargate(state, run).await {
            Ok(arn) => return Ok(arn),
            Err(e) => {
                tracing::warn!(
                    "Failed to launch Fargate on AWS: {}. Falling back to local process spawning.",
                    e
                );
            }
        }
    }

    launch_local_process(state, run).await
}

async fn launch_aws_fargate(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    let sdk_config = aws_config::load_defaults(aws_config::BehaviorVersion::latest()).await;
    let client = EcsClient::new(&sdk_config);
    let callback_url = format!("{}/api/internal/callback", state.config.server.external_url);

    // デフォルトの設定をロード
    let mut cluster_arn = state.config.fargate.cluster_arn.clone();
    let mut task_definition = state.config.fargate.task_definition.clone();
    let mut subnets = state.config.fargate.subnets.clone();
    let mut security_groups = state.config.fargate.security_groups.clone();
    let mut container_name = state.config.fargate.container_name.clone();
    let mut assign_public_ip_bool = state.config.fargate.assign_public_ip.unwrap_or(true);

    // 自作ワーカー定義の設定でオーバーライド
    if let Some(def_id) = run.worker_definition_id
        && let Some(def) = crate::db::workers::get_worker_definition(&state.db, &def_id).await?
    {
        if let Some(val) = def.config.get("cluster_arn").and_then(|v| v.as_str()) {
            cluster_arn = val.to_string();
        }
        if let Some(val) = def.config.get("task_definition").and_then(|v| v.as_str()) {
            task_definition = val.to_string();
        }
        if let Some(val) = def.config.get("subnets").and_then(|v| v.as_array()) {
            subnets = val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(val) = def.config.get("security_groups").and_then(|v| v.as_array()) {
            security_groups = val
                .iter()
                .filter_map(|v| v.as_str().map(|s| s.to_string()))
                .collect();
        }
        if let Some(val) = def.config.get("container_name").and_then(|v| v.as_str()) {
            container_name = val.to_string();
        }
        if let Some(val) = def.config.get("assign_public_ip").and_then(|v| v.as_bool()) {
            assign_public_ip_bool = val;
        }
    }

    let container_override = ContainerOverride::builder()
        .name(&container_name)
        .command("mrs-harris")
        .command("worker")
        .command("--task-id")
        .command(run.id.to_string())
        .command("--callback-url")
        .command(&callback_url)
        .build();

    let task_override = TaskOverride::builder()
        .container_overrides(container_override)
        .build();

    let assign_public_ip = if assign_public_ip_bool {
        AssignPublicIp::Enabled
    } else {
        AssignPublicIp::Disabled
    };

    let vpc_config = AwsVpcConfiguration::builder()
        .set_subnets(Some(subnets))
        .set_security_groups(Some(security_groups))
        .assign_public_ip(assign_public_ip)
        .build()?;

    let network_config = NetworkConfiguration::builder()
        .awsvpc_configuration(vpc_config)
        .build();

    let response = client
        .run_task()
        .cluster(&cluster_arn)
        .task_definition(&task_definition)
        .launch_type(LaunchType::Fargate)
        .overrides(task_override)
        .network_configuration(network_config)
        .send()
        .await?;

    let task_arn = response
        .tasks()
        .first()
        .ok_or_else(|| anyhow::anyhow!("No task was launched"))?
        .task_arn()
        .ok_or_else(|| anyhow::anyhow!("Launched task has no ARN"))?
        .to_string();

    tracing::info!("Fargate task launched successfully on AWS: {}", task_arn);
    Ok(task_arn)
}

async fn launch_local_process(state: &AppState, run: &JobRun) -> anyhow::Result<String> {
    crate::worker_manager::local_fallback::launch(state, run, "local-fargate").await
}
