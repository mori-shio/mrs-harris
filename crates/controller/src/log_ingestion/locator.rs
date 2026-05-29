use anyhow::{Result, anyhow};

use crate::app::AppState;

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct CloudWatchLogTarget {
    pub region: String,
    pub log_group_name: String,
    pub log_stream_name: Option<String>,
    pub stream_prefix: Option<String>,
    pub start_time_hint_ms: Option<i64>,
}

pub trait CloudWatchStreamLocator: Send + Sync {
    fn resolve(
        &self,
        _state: &AppState,
        _run: &mrs_harris_common::models::run::JobRun,
    ) -> Result<CloudWatchLogTarget>;
}

pub struct FargateStreamLocator;

impl FargateStreamLocator {
    pub fn from_config(config: &serde_json::Value) -> Result<CloudWatchLogTarget> {
        let log_group_name = config
            .get("log_group_name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("missing log_group_name"))?;

        Ok(CloudWatchLogTarget {
            region: config
                .get("aws_region")
                .and_then(|value| value.as_str())
                .unwrap_or("ap-northeast-1")
                .to_string(),
            log_group_name: log_group_name.to_string(),
            log_stream_name: None,
            stream_prefix: config
                .get("log_stream_prefix")
                .and_then(|value| value.as_str())
                .map(str::to_string),
            start_time_hint_ms: Some(chrono::Utc::now().timestamp_millis()),
        })
    }
}

pub struct LambdaStreamLocator;

impl LambdaStreamLocator {
    pub fn from_config(config: &serde_json::Value) -> Result<CloudWatchLogTarget> {
        let log_group_name = config
            .get("log_group_name")
            .and_then(|value| value.as_str())
            .ok_or_else(|| anyhow!("missing log_group_name"))?;

        Ok(CloudWatchLogTarget {
            region: config
                .get("aws_region")
                .and_then(|value| value.as_str())
                .unwrap_or("ap-northeast-1")
                .to_string(),
            log_group_name: log_group_name.to_string(),
            log_stream_name: None,
            stream_prefix: config
                .get("function_name")
                .and_then(|value| value.as_str())
                .map(|name| format!("{name}:")),
            start_time_hint_ms: Some(chrono::Utc::now().timestamp_millis()),
        })
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn fargate_locator_reads_worker_definition_logging_config() {
        let target = FargateStreamLocator::from_config(&serde_json::json!({
            "aws_region": "ap-northeast-1",
            "log_group_name": "/ecs/mrs-harris",
            "log_stream_prefix": "worker/app"
        }))
        .unwrap();

        assert_eq!(target.region, "ap-northeast-1");
        assert_eq!(target.log_group_name, "/ecs/mrs-harris");
        assert_eq!(target.stream_prefix.as_deref(), Some("worker/app"));
        assert!(target.log_stream_name.is_none());
    }

    #[test]
    fn lambda_locator_reads_function_logging_config() {
        let target = LambdaStreamLocator::from_config(&serde_json::json!({
            "aws_region": "ap-northeast-1",
            "log_group_name": "/aws/lambda/mrs-harris-worker",
            "function_name": "mrs-harris-worker"
        }))
        .unwrap();

        assert_eq!(target.region, "ap-northeast-1");
        assert_eq!(target.log_group_name, "/aws/lambda/mrs-harris-worker");
        assert_eq!(target.stream_prefix.as_deref(), Some("mrs-harris-worker:"));
    }
}
