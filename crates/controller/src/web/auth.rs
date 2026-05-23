use axum::{
    extract::{State, FromRequestParts},
    http::{request::Parts, StatusCode},
    response::{IntoResponse, Redirect, Response},
    routing::{get, post},
    Form, Router,
};
use axum_extra::extract::cookie::{Cookie, CookieJar};
use time;
use askama::Template;
use mrs_harris_common::models::user::{Claims, LoginRequest};
use crate::app::AppState;
use argon2::{Argon2, PasswordVerifier};
use jsonwebtoken::{encode, decode, Header, EncodingKey, DecodingKey, Validation};
use std::time::SystemTime;

/// Web用カスタム認証抽出子。JWTがCookieに存在しない、または無効な場合は自動的に `/login` にリダイレクトします。
pub struct WebClaims(pub Claims);

impl FromRequestParts<AppState> for WebClaims {
    type Rejection = Redirect;

    async fn from_request_parts(
        parts: &mut Parts,
        state: &AppState,
    ) -> Result<Self, Self::Rejection> {
        let cookie_header = parts
            .headers
            .get(axum::http::header::COOKIE)
            .and_then(|value| value.to_str().ok());

        let token = cookie_header.and_then(|cookie_str| {
            cookie_str
                .split(';')
                .map(|c| c.trim())
                .find(|c| c.starts_with("jwt="))
                .map(|c| c[4..].to_string())
        });

        let token_str = match token {
            Some(t) => t,
            None => return Err(Redirect::to("/login")),
        };

        let token_data = decode::<Claims>(
            &token_str,
            &DecodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
            &Validation::default(),
        );

        match token_data {
            Ok(data) => Ok(WebClaims(data.claims)),
            Err(_) => Err(Redirect::to("/login")),
        }
    }
}

#[derive(Template)]
#[template(path = "login.html")]
struct LoginTemplate {
    error: Option<String>,
    username: String,
}
crate::impl_into_response!(LoginTemplate);


pub fn router() -> Router<AppState> {
    Router::new()
        .route("/login", get(login_page).post(login_submit))
        .route("/logout", get(logout))
}

async fn login_page() -> impl IntoResponse {
    LoginTemplate {
        error: None,
        username: String::new(),
    }
}

async fn login_submit(
    State(state): State<AppState>,
    jar: CookieJar,
    Form(payload): Form<LoginRequest>,
) -> impl IntoResponse {
    // 1. DBからユーザーを取得
    let user_opt = match crate::db::users::get_user_by_username(&state.db, &payload.username).await {
        Ok(opt) => opt,
        Err(e) => {
            return LoginTemplate {
                error: Some(format!("データベースエラー: {}", e)),
                username: payload.username,
            }
            .into_response();
        }
    };

    let user = match user_opt {
        Some(u) => u,
        None => {
            return LoginTemplate {
                error: Some("ユーザー名またはパスワードが正しくありません。".to_string()),
                username: payload.username,
            }
            .into_response();
        }
    };

    // 2. パスワードの検証
    let parsed_hash = match argon2::PasswordHash::new(&user.password_hash) {
        Ok(hash) => hash,
        Err(e) => {
            return LoginTemplate {
                error: Some(format!("パスワードハッシュ検証エラー: {}", e)),
                username: payload.username,
            }
            .into_response();
        }
    };

    let argon2 = Argon2::default();
    if argon2
        .verify_password(payload.password.as_bytes(), &parsed_hash)
        .is_err()
    {
        return LoginTemplate {
            error: Some("ユーザー名またはパスワードが正しくありません。".to_string()),
            username: payload.username,
        }
        .into_response();
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

    let token = match encode(
        &Header::default(),
        &claims,
        &EncodingKey::from_secret(state.config.auth.jwt_secret.as_bytes()),
    ) {
        Ok(t) => t,
        Err(e) => {
            return LoginTemplate {
                error: Some(format!("JWT生成エラー: {}", e)),
                username: payload.username,
            }
            .into_response();
        }
    };

    // 4. Cookieにトークンをセットしてリダイレクト
    let mut cookie = Cookie::new("jwt", token);
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_max_age(time::Duration::hours(state.config.auth.jwt_expiry_hours as i64));

    let updated_jar = jar.add(cookie);
    (updated_jar, Redirect::to("/")).into_response()
}

async fn logout(jar: CookieJar) -> impl IntoResponse {
    let mut cookie = Cookie::new("jwt", "");
    cookie.set_path("/");
    cookie.set_http_only(true);
    cookie.set_max_age(time::Duration::ZERO); // 即座に削除

    let updated_jar = jar.add(cookie);
    (updated_jar, Redirect::to("/login")).into_response()
}
