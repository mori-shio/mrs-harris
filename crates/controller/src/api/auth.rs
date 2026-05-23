use axum::{
    extract::{State, FromRequestParts},
    http::{StatusCode, request::Parts},
    routing::post,
    Json, Router,
};
use mrs_harris_common::models::user::{LoginRequest, Claims};
use crate::app::AppState;
use argon2::{Argon2, PasswordVerifier};
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation};
use std::time::SystemTime;

pub fn router() -> Router<AppState> {
    Router::new().route("/auth/login", post(login))
}

async fn login(
    State(state): State<AppState>,
    Json(payload): Json<LoginRequest>,
) -> Result<Json<serde_json::Value>, (StatusCode, Json<serde_json::Value>)> {
    // 1. DBからユーザーを取得
    let user_opt = crate::db::users::get_user_by_username(&state.db, &payload.username)
        .await
        .map_err(|e| {
            (
                StatusCode::INTERNAL_SERVER_ERROR,
                Json(serde_json::json!({ "error": format!("Database error: {}", e) })),
            )
        })?;

    let user = match user_opt {
        Some(u) => u,
        None => {
            return Err((
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Invalid username or password" })),
            ));
        }
    };

    // 2. パスワードハッシュの検証
    let parsed_hash = argon2::PasswordHash::new(&user.password_hash).map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("Invalid password hash in DB: {}", e) })),
        )
    })?;

    let argon2 = Argon2::default();
    if argon2
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return Err((
            StatusCode::UNAUTHORIZED,
            Json(serde_json::json!({ "error": "Invalid username or password" })),
        ));
    }

    // 3. JWTトークンの生成
    let iat = SystemTime::now()
        .duration_since(SystemTime::UNIX_EPOCH)
        .unwrap()
        .as_secs() as usize;
    let exp = iat + (state.config.auth.jwt_expiry_hours as usize * 3600);

    let claims = Claims {
        sub: user.id,
        username: user.username.clone(),
        role: user.role.clone(),
        exp,
        iat,
    };

    let token = encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    )
    .map_err(|e| {
        (
            StatusCode::INTERNAL_SERVER_ERROR,
            Json(serde_json::json!({ "error": format!("JWT encoding error: {}", e) })),
        )
    })?;

    Ok(Json(serde_json::json!({
        "token": token,
        "role": user.role.to_string(),
    })))
}

impl FromRequestParts<AppState> for Claims {
    type Rejection = (StatusCode, Json<serde_json::Value>);

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        // 1. Authorizationヘッダーから抽出
        let auth_header = parts
            .headers
            .get(axum::http::header::AUTHORIZATION)
            .and_then(|value| value.to_str().ok());

        let token = if let Some(header_str) = auth_header {
            if header_str.starts_with("Bearer ") {
                Some(header_str[7..].to_string())
            } else {
                None
            }
        } else {
            // 2. Cookieから抽出 (ダッシュボード用)
            parts
                .headers
                .get(axum::http::header::COOKIE)
                .and_then(|value| value.to_str().ok())
                .and_then(|cookie_str| {
                    cookie_str
                        .split(';')
                        .map(|c| c.trim())
                        .find(|c| c.starts_with("jwt="))
                        .map(|c| c[4..].to_string())
                })
        };

        let token_str = token.ok_or_else(|| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": "Missing authorization token" })),
            )
        })?;

        // 3. デコードと検証
        let token_data = decode::<Claims>(
            &token_str,
            &DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
            &Validation::default(),
        )
        .map_err(|e| {
            (
                StatusCode::UNAUTHORIZED,
                Json(serde_json::json!({ "error": format!("Invalid token: {}", e) })),
            )
        })?;

        Ok(token_data.claims)
    }
}
