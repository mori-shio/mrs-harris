use askama::Template;
use axum::{
    Form, Router,
    extract::State,
    response::IntoResponse,
    routing::{get, post},
};
use std::collections::HashMap;
use std::sync::Arc;

use super::auth::WebClaims;
use crate::app::AppState;
use mrs_harris_common::config::ControllerConfig;
use sqlx::Row;

#[derive(Template)]
#[template(path = "settings.html")]
struct SettingsTemplate {
    config: Arc<ControllerConfig>,
    database_url_masked: String,
    fargate_subnets: String,
    fargate_sgs: String,
    slack_webhook_url: String,
    slack_ssm_path: String,
    slack_ssm_region: String,
}
crate::impl_into_response!(SettingsTemplate);

pub fn router() -> Router<AppState> {
    Router::new()
        .route("/settings", get(settings_page))
        .route("/settings/test-email", post(test_email_submit))
        .route("/settings/slack", post(save_slack_settings_submit))
}

async fn settings_page(_claims: WebClaims, State(state): State<AppState>) -> impl IntoResponse {
    let config = state.config.clone();

    // DB URLをマスク
    let database_url_masked = mask_db_url(&config.database.url);

    // Fargate サブネットとセキュリティグループを文字列化
    let fargate_subnets = config.fargate.subnets.join(", ");
    let fargate_sgs = config.fargate.security_groups.join(", ");

    // Slack 設定の取得
    let slack_row = sqlx::query("SELECT config FROM notification_channels WHERE id = 'c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d'")
        .fetch_optional(&state.db)
        .await
        .unwrap_or_default();

    let mut slack_webhook_url = String::new();
    let mut slack_ssm_path = String::new();
    let mut slack_ssm_region = String::new();

    if let Some(row) = slack_row
        && let Ok(config_val) = row.try_get::<serde_json::Value, _>("config")
    {
        if let Some(url) = config_val.get("webhook_url").and_then(|v| v.as_str()) {
            slack_webhook_url = url.to_string();
        }
        if let Some(path) = config_val
            .get("ssm_parameter_path")
            .and_then(|v| v.as_str())
        {
            slack_ssm_path = path.to_string();
        }
        if let Some(region) = config_val.get("ssm_region").and_then(|v| v.as_str()) {
            slack_ssm_region = region.to_string();
        }
    }

    SettingsTemplate {
        config,
        database_url_masked,
        fargate_subnets,
        fargate_sgs,
        slack_webhook_url,
        slack_ssm_path,
        slack_ssm_region,
    }
}

async fn test_email_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Form(payload): Form<HashMap<String, String>>,
) -> impl IntoResponse {
    let to_address = payload.get("to_address").cloned().unwrap_or_default();

    if to_address.is_empty() {
        return r#"<div style="background: rgba(239, 68, 68, 0.08); border: 1px solid rgba(239, 68, 68, 0.2); border-radius: 8px; padding: 12px; color: #fca5a5; font-size: 0.9rem;">
            宛先アドレスが空です。
        </div>"#.into_response();
    }

    let email_config = match &state.config.notification.email {
        Some(cfg) => cfg,
        None => {
            return r#"<div style="background: rgba(239, 68, 68, 0.08); border: 1px solid rgba(239, 68, 68, 0.2); border-radius: 8px; padding: 12px; color: #fca5a5; font-size: 0.9rem;">
                SMTP メール設定が定義されていません。
            </div>"#.into_response();
        }
    };

    // メールテスト送信を実行
    match crate::notification::email::send_test_email(email_config, &to_address).await {
        Ok(_) => {
            r#"<div style="background: rgba(16, 185, 129, 0.08); border: 1px solid rgba(16, 185, 129, 0.2); border-radius: 8px; padding: 12px; color: #a7f3d0; font-size: 0.9rem; display: flex; align-items: center; gap: 8px;">
                <i data-lucide="check-circle" style="width: 18px; height: 18px; color: var(--color-success);"></i>
                テストメールが正常に送信されました！宛先メールボックスをご確認ください。
            </div>
            <script>lucide.createIcons();</script>"#.into_response()
        }
        Err(e) => {
            let err_msg = format!("テストメールの送信に失敗しました: {}", e);
            format!(
                r#"<div style="background: rgba(239, 68, 68, 0.08); border: 1px solid rgba(239, 68, 68, 0.2); border-radius: 8px; padding: 12px; color: #fca5a5; font-size: 0.9rem; display: flex; flex-direction: column; gap: 6px;">
                    <div style="display: flex; align-items: center; gap: 8px;">
                        <i data-lucide="alert-circle" style="width: 18px; height: 18px; color: var(--color-danger);"></i>
                        <strong>送信エラー</strong>
                    </div>
                    <p style="font-family: 'JetBrains Mono', monospace; font-size: 0.8rem; margin: 0; padding-left: 26px; word-break: break-all;">{}</p>
                </div>
                <script>lucide.createIcons();</script>"#,
                err_msg
            ).into_response()
        }
    }
}

fn mask_db_url(url: &str) -> String {
    if let Some(proto_pos) = url.find("://") {
        let auth_start = proto_pos + 3;
        if let Some(at_pos) = url[auth_start..].find('@') {
            let actual_at = auth_start + at_pos;
            let auth_part = &url[auth_start..actual_at];
            if let Some(colon_pos) = auth_part.find(':') {
                let user = &auth_part[..colon_pos];
                return format!("{}://{}:****{}", &url[..proto_pos], user, &url[actual_at..]);
            }
        }
    }
    // パースできない場合は安全のため全体を伏せる
    "mysql://****:****@****/****".to_string()
}

#[derive(serde::Deserialize)]
pub struct SlackSettingsForm {
    webhook_url: String,
    ssm_parameter_path: String,
    ssm_region: String,
}

async fn save_slack_settings_submit(
    _claims: WebClaims,
    State(state): State<AppState>,
    Form(form): Form<SlackSettingsForm>,
) -> impl IntoResponse {
    let config = serde_json::json!({
        "webhook_url": form.webhook_url.trim(),
        "ssm_parameter_path": form.ssm_parameter_path.trim(),
        "ssm_region": form.ssm_region.trim(),
    });

    let res = sqlx::query(
        r#"INSERT INTO notification_channels (id, name, channel_type, config, is_active)
           VALUES ('c1a2b3c4-e5f6-7a8b-9c0d-1e2f3a4b5c6d', 'default-slack', 'slack', ?, 1)
           ON DUPLICATE KEY UPDATE config = ?, is_active = 1"#,
    )
    .bind(&config)
    .bind(&config)
    .execute(&state.db)
    .await;

    match res {
        Ok(_) => {
            r#"<div id="slack-save-result" style="background: rgba(16, 185, 129, 0.08); border: 1px solid rgba(16, 185, 129, 0.2); border-radius: 8px; padding: 12px; color: #a7f3d0; font-size: 0.9rem; display: flex; align-items: center; gap: 8px; margin-top: 16px;">
                <i data-lucide="check-circle" style="width: 18px; height: 18px; color: var(--color-success);"></i>
                Slack設定が正常に保存されました！
            </div>
            <script>lucide.createIcons();</script>"#.into_response()
        }
        Err(e) => {
            let err_msg = format!("保存エラー: {}", e);
            format!(
                r#"<div id="slack-save-result" style="background: rgba(239, 68, 68, 0.08); border: 1px solid rgba(239, 68, 68, 0.2); border-radius: 8px; padding: 12px; color: #fca5a5; font-size: 0.9rem; display: flex; align-items: center; gap: 8px; margin-top: 16px;">
                    <i data-lucide="alert-circle" style="width: 18px; height: 18px; color: var(--color-danger);"></i>
                    {}
                </div>
                <script>lucide.createIcons();</script>"#,
                err_msg
            ).into_response()
        }
    }
}
