use mrs_harris_common::config::EmailGlobalConfig;
use mrs_harris_common::models::notification::EmailConfig;
use lettre::transport::smtp::authentication::Credentials;
use lettre::{Message, SmtpTransport, Transport};

/// テストメールを送信
pub async fn send_test_email(
    config: &EmailGlobalConfig,
    to_address: &str,
) -> anyhow::Result<()> {
    let email = Message::builder()
        .from(config.from_address.parse()?)
        .to(to_address.parse()?)
        .subject("Mrs. Harris SMTP テスト送信")
        .body(format!(
            "このメールは Mrs. Harris ジョブスケジューラからのSMTP接続テストメールです。\n\n送信時刻: {}\nステータス: 正常に送信されました。\n",
            chrono::Local::now().format("%Y-%m-%d %H:%M:%S")
        ))?;

    // SmtpTransport を構築
    let mut transport_builder = SmtpTransport::builder_dangerous(&config.smtp_host)
        .port(config.smtp_port);

    if let (Some(user), Some(pass)) = (&config.username, &config.password) {
        if !user.is_empty() && !pass.is_empty() {
            let creds = Credentials::new(user.clone(), pass.clone());
            transport_builder = transport_builder.credentials(creds);
        }
    }

    let transport = transport_builder.build();

    // lettre の同期送信を非同期コンテキストで安全に実行
    tokio::task::spawn_blocking(move || {
        transport.send(&email)
    })
    .await??;

    Ok(())
}

/// メール通知を送信する
pub async fn send_email_notification(
    global_config: &EmailGlobalConfig,
    email_config: &EmailConfig,
    subject: &str,
    body: &str,
) -> anyhow::Result<()> {
    // SmtpTransport を構築
    let mut transport_builder = SmtpTransport::builder_dangerous(&global_config.smtp_host)
        .port(global_config.smtp_port);

    if let (Some(user), Some(pass)) = (&global_config.username, &global_config.password) {
        if !user.is_empty() && !pass.is_empty() {
            let creds = Credentials::new(user.clone(), pass.clone());
            transport_builder = transport_builder.credentials(creds);
        }
    }

    let transport = transport_builder.build();

    for to_addr in &email_config.to {
        let mut builder = Message::builder()
            .from(global_config.from_address.parse()?)
            .to(to_addr.parse()?)
            .subject(subject);

        if let Some(ref cc_list) = email_config.cc {
            for cc_addr in cc_list {
                builder = builder.cc(cc_addr.parse()?);
            }
        }

        let email = builder.body(body.to_string())?;

        let transport_clone = transport.clone();
        tokio::task::spawn_blocking(move || {
            transport_clone.send(&email)
        })
        .await??;
    }

    Ok(())
}
