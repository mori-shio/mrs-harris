use chrono::{DateTime, Utc};
use serde::{Deserialize, Serialize};


use super::run::RunStatus;

/// カレンダー表示用エントリ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEntry {
    pub run_id: i64,
    pub job_id: i64,
    pub job_name: String,
    pub status: RunStatus,
    pub scheduled_at: Option<DateTime<Utc>>,
    pub started_at: Option<DateTime<Utc>>,
    pub finished_at: Option<DateTime<Utc>>,
    pub duration_ms: Option<i64>,
}

/// FullCalendar.js 用のイベント形式
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEvent {
    pub id: String,
    pub title: String,
    /// ISO 8601 形式
    pub start: String,
    /// ISO 8601 形式
    pub end: Option<String>,
    /// ステータスに応じた色
    pub color: String,
    #[serde(rename = "extendedProps")]
    pub extended_props: CalendarEventProps,
}

/// カレンダーイベントの拡張プロパティ
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CalendarEventProps {
    pub run_id: String,
    pub job_id: String,
    pub status: String,
    pub duration_ms: Option<i64>,
}

/// カレンダークエリパラメータ
#[derive(Debug, Clone, Deserialize)]
pub struct CalendarQuery {
    pub start: DateTime<Utc>,
    pub end: DateTime<Utc>,
}

impl CalendarEntry {
    /// FullCalendar.js イベント形式に変換
    pub fn to_calendar_event(&self) -> CalendarEvent {
        let color = match self.status {
            RunStatus::Succeeded => "#10b981".to_string(),                      // 緑
            RunStatus::Failed | RunStatus::DeadLetter => "#ef4444".to_string(), // 赤
            RunStatus::Running => "#f59e0b".to_string(),                        // 黄
            RunStatus::Retrying => "#8b5cf6".to_string(),                       // 紫
            RunStatus::Cancelled => "#6b7280".to_string(),                      // 灰
            _ => "#3b82f6".to_string(),                                         // 青
        };

        let start_time = self
            .started_at
            .or(self.scheduled_at)
            .unwrap_or(chrono::Utc::now());

        CalendarEvent {
            id: self.run_id.to_string(),
            title: self.job_name.clone(),
            start: start_time.to_rfc3339(),
            end: self.finished_at.map(|t| t.to_rfc3339()),
            color,
            extended_props: CalendarEventProps {
                run_id: self.run_id.to_string(),
                job_id: self.job_id.to_string(),
                status: self.status.to_string(),
                duration_ms: self.duration_ms,
            },
        }
    }
}
