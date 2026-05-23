use mrs_harris_common::models::notification::SlackConfig;
use serde::Serialize;

#[derive(Serialize)]
struct SlackPayload {
    text: String,
    #[serde(skip_serializing_if = "Option::is_none")]
    channel: Option<String>,
    #[serde(skip_serializing_if = "Option::is_none")]
    username: Option<String>,
}

/// Slack Webhook に通知を送信する
pub async fn send_slack_notification(
    config: &SlackConfig,
    text: &str,
) -> anyhow::Result<()> {
    let client = reqwest::Client::new();
    let payload = SlackPayload {
        text: text.to_string(),
        channel: config.channel.clone(),
        username: config.username.clone(),
    };

    let response = client
        .post(&config.webhook_url)
        .json(&payload)
        .send()
        .await?;

    if !response.status().is_success() {
        let err_body = response.text().await?;
        return Err(anyhow::anyhow!(
            "Slack Webhook returned error: {} - {}",
            payload.channel.unwrap_or_default(),
            err_body
        ));
    }

    Ok(())
}
