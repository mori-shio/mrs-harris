use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};

#[derive(
    Debug, Clone, PartialEq, Eq, Serialize, Deserialize, strum::Display, strum::EnumString,
)]
#[strum(serialize_all = "snake_case")]
#[serde(rename_all = "snake_case")]
pub enum StepFlowRunCondition {
    OnSuccess,
    Always,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepFlowGroup {
    pub id: Option<i64>,
    pub step_flow_id: Option<i64>,
    pub group_order: u32,
    pub run_condition: Option<StepFlowRunCondition>,
    pub steps: Vec<StepFlowStep>,
}

impl StepFlowGroup {
    pub fn validate_run_condition(&self) -> Result<(), String> {
        match (self.group_order, &self.run_condition) {
            (1, None) => {
                if self.steps.is_empty() {
                    Err("group must have at least one step".to_string())
                } else {
                    Ok(())
                }
            }
            (1, Some(_)) => Err("group 1 must not have run_condition".to_string()),
            (_, Some(_)) => {
                if self.steps.is_empty() {
                    Err("group must have at least one step".to_string())
                } else {
                    Ok(())
                }
            }
            (_, None) => Err("group 2 and later require run_condition".to_string()),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct StepFlowStep {
    pub id: Option<i64>,
    pub group_id: Option<i64>,
    pub step_order: u32,
    pub job_id: i64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFlow {
    pub id: i64,
    pub name: String,
    pub description: Option<String>,
    pub space_id: Option<i64>,
    pub is_active: bool,
    pub timeout_sec: u32,
    pub tags: Vec<String>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct NewStepFlow {
    pub name: String,
    pub description: Option<String>,
    pub space_id: Option<i64>,
    pub is_active: bool,
    pub timeout_sec: u32,
    pub tags: Vec<String>,
    pub groups: Vec<StepFlowGroup>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFlowHistoryEntry {
    pub id: i64,
    pub step_flow_id: i64,
    pub version: u32,
    pub payload: serde_json::Value,
    pub changed_by: String,
    pub changed_at: DateTime<Utc>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StepFlowRun {
    pub id: i64,
    pub step_flow_id: i64,
    pub step_flow_history_id: i64,
    pub run_number: i64,
    pub status: crate::models::run::RunStatus,
    pub trigger_type: crate::models::run::TriggerType,
    pub created_by: String,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
    pub created_at: DateTime<Utc>,
    pub updated_at: DateTime<Utc>,
}

#[cfg(test)]
mod tests {
    use super::{StepFlowGroup, StepFlowRunCondition};

    #[test]
    fn group_run_condition_is_required_only_after_first_group() {
        assert!(
            StepFlowGroup {
                id: None,
                step_flow_id: None,
                group_order: 1,
                run_condition: None,
                steps: vec![super::StepFlowStep {
                    id: None,
                    group_id: None,
                    step_order: 1,
                    job_id: 1,
                }],
            }
            .validate_run_condition()
            .is_ok()
        );

        assert!(
            StepFlowGroup {
                id: None,
                step_flow_id: None,
                group_order: 1,
                run_condition: Some(StepFlowRunCondition::Always),
                steps: vec![super::StepFlowStep {
                    id: None,
                    group_id: None,
                    step_order: 1,
                    job_id: 1,
                }],
            }
            .validate_run_condition()
            .unwrap_err()
            .contains("group 1")
        );

        assert!(
            StepFlowGroup {
                id: None,
                step_flow_id: None,
                group_order: 2,
                run_condition: Some(StepFlowRunCondition::OnSuccess),
                steps: vec![super::StepFlowStep {
                    id: None,
                    group_id: None,
                    step_order: 1,
                    job_id: 1,
                }],
            }
            .validate_run_condition()
            .is_ok()
        );

        assert!(
            StepFlowGroup {
                id: None,
                step_flow_id: None,
                group_order: 2,
                run_condition: None,
                steps: vec![super::StepFlowStep {
                    id: None,
                    group_id: None,
                    step_order: 1,
                    job_id: 1,
                }],
            }
            .validate_run_condition()
            .unwrap_err()
            .contains("require run_condition")
        );
    }
}
