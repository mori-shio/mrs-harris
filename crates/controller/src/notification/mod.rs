pub mod email;
pub mod slack;

use crate::app::AppState;
use mrs_harris_common::models::notification::{ChannelType, SlackConfig, EmailConfig};

use chrono::Utc;

/// ジョブのステータス変更イベントが発生した際、登録されたチャネルへ通知を送信する
pub async fn trigger_notifications(
    state: &AppState,
    run_id: &i64,
    event: &str, // e.g. "succeeded", "failed", "dead_letter"
) -> anyhow::Result<()> {
    // 1. 実行履歴を取得
    let run = match crate::db::runs::get_run(&state.db, run_id).await? {
        Some(r) => r,
        None => {
            tracing::warn!("Notification triggered for non-existing run: {}", run_id);
            return Ok(());
        }
    };

    // 2. ジョブ定義を取得
    let job = match crate::db::jobs::get_job(&state.db, &run.job_id).await? {
        Some(j) => j,
        None => {
            tracing::warn!("Notification triggered for run {} with non-existing job: {}", run_id, run.job_id);
            return Ok(());
        }
    };

    // 3. イベントに対応する通知チャネルをDBから取得
    let channels = crate::db::notifications::get_notifications_for_job(&state.db, &job.id, event).await?;
    if channels.is_empty() {
        return Ok(());
    }

    tracing::info!("Sending {} notification(s) for job {} ({}), event: {}", channels.len(), job.name, run_id, event);

    // 4. メッセージ文字列の作成
    let duration_sec = run.duration_ms.map(|d| d as f64 / 1000.0).unwrap_or(0.0);
    let start_time_str = run.started_at.map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_else(|| "N/A".to_string());
    let end_time_str = run.finished_at.map(|t| t.with_timezone(&chrono::Local).format("%Y-%m-%d %H:%M:%S").to_string()).unwrap_or_else(|| "N/A".to_string());
    
    let status_emoji = match event {
        "succeeded" => "✅",
        "failed" => "❌",
        "dead_letter" => "💀",
        _ => "📢",
    };

    let subject = format!("[Mrs. Harris] {} ジョブ '{}' が {} しました", status_emoji, job.name, translate_event(event));
    
    let mut body = format!(
        "🔔 Mrs. Harris ジョブ実行通知\n\n\
         ========================================\n\
         ジョブ名      : {}\n\
         ジョブタイプ  : {}\n\
         実行ステータス: {} ({})\n\
         実行 ID       : {}\n\
         ワーカー種別  : {}\n\
         開始時刻      : {}\n\
         終了時刻      : {}\n\
         実行時間      : {:.2} 秒\n",
        job.name,
        job.job_type,
        translate_event(event),
        event,
        run_id,
        run.worker_type,
        start_time_str,
        end_time_str,
        duration_sec
    );

    if let Some(ref err) = run.error {
        body.push_str(&format!("エラー詳細    : {}\n", err));
    }
    body.push_str("========================================\n");

    // 5. 各チャネルに通知をディスパッチ
    for channel in channels {
        match channel.channel_type {
            ChannelType::Slack => {
                let mut slack_config: SlackConfig = serde_json::from_value(channel.config)?;
                
                // SSM Parameter Store から webhook_url を動的取得する設定がある場合
                if let Some(ref path) = slack_config.ssm_parameter_path {
                    if !path.trim().is_empty() {
                        let region = slack_config.ssm_region.as_deref().unwrap_or("ap-northeast-1");
                        tracing::info!("SSM パラメータストアから Slack Webhook URL を取得します: {} (リージョン: {})", path, region);
                        match fetch_ssm_parameter(region, path).await {
                            Ok(url) => {
                                slack_config.webhook_url = url.trim().to_string();
                            }
                            Err(e) => {
                                tracing::error!("SSM パラメータストアからの Slack Webhook URL 取得に失敗しました: {}. 設定済みの固定 URL でフォールバックします。", e);
                            }
                        }
                    }
                }

                // Webhook URL がチャネル設定になければグローバル設定を試みる
                if slack_config.webhook_url.is_empty() {
                    if let Some(ref global_slack) = state.config.notification.slack {
                        if let Some(ref url) = global_slack.default_webhook_url {
                            slack_config.webhook_url = url.clone();
                        }
                    }
                }

                if slack_config.webhook_url.is_empty() {
                    tracing::error!("Slack notification failed: Webhook URL is empty for channel: {}", channel.name);
                    continue;
                }

                if let Err(e) = slack::send_slack_notification(&slack_config, &body).await {
                    tracing::error!("Failed to send Slack notification for channel '{}': {}", channel.name, e);
                }
            }
            ChannelType::Email => {
                let email_config: EmailConfig = serde_json::from_value(channel.config)?;
                if let Some(ref global_email) = state.config.notification.email {
                    if let Err(e) = email::send_email_notification(global_email, &email_config, &subject, &body).await {
                        tracing::error!("Failed to send Email notification for channel '{}': {}", channel.name, e);
                    }
                } else {
                    tracing::error!("Email notification failed: Global SMTP configuration is not available");
                }
            }
        }
    }

    Ok(())
}

fn translate_event(event: &str) -> &str {
    match event {
        "succeeded" => "成功",
        "failed" => "失敗",
        "dead_letter" => "デッドレター移行 (リトライ上限超え)",
        "running" => "実行中",
        _ => event,
    }
}

async fn fetch_ssm_parameter(region: &str, path: &str) -> anyhow::Result<String> {
    use aws_config::BehaviorVersion;
    let config = aws_config::defaults(BehaviorVersion::latest())
        .region(aws_config::Region::new(region.to_string()))
        .load()
        .await;
    let client = aws_sdk_ssm::Client::new(&config);
    let resp = client.get_parameter()
        .name(path)
        .with_decryption(true)
        .send()
        .await?;
    let val = resp.parameter()
        .and_then(|p| p.value())
        .ok_or_else(|| anyhow::anyhow!("Parameter is empty"))?
        .to_string();
    Ok(val)
}
