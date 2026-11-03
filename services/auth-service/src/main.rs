use std::{env, fs, net::SocketAddr, time::Duration};

use anyhow::Context;
use argon2::{
    password_hash::{PasswordHash, PasswordHasher, PasswordVerifier, SaltString},
    Argon2,
};
use axum::{
    extract::State,
    http::StatusCode,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use platform_auth::{JwtSigner, Role};
use rand::rngs::OsRng;
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

const ACCESS_TOKEN_LIFETIME: Duration = Duration::from_secs(900);

#[derive(Clone)]
struct AppState {
    pool: PgPool,
    signer: JwtSigner,
    dummy_password_hash: String,
}

#[derive(Deserialize)]
struct LoginRequest {
    email: String,
    password: String,
}

#[derive(FromRow)]
struct LoginUser {
    id: Uuid,
    tenant_id: Uuid,
    role: String,
    password_hash: String,
    is_active: bool,
}

#[derive(Serialize)]
struct LoginResponse {
    access_token: String,
    token_type: &'static str,
    expires_at: usize,
    user: LoginUserResponse,
}

#[derive(Serialize)]
struct LoginUserResponse {
    id: Uuid,
    tenant_id: Uuid,
    role: Role,
}

#[derive(Debug)]
struct ApiError(anyhow::Error);

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        tracing::error!(error = %self.0, "auth request failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "auth service error").into_response()
    }
}

fn hash_password(password: &str) -> anyhow::Result<String> {
    let salt = SaltString::generate(&mut OsRng);
    Argon2::default()
        .hash_password(password.as_bytes(), &salt)
        .map(|hash| hash.to_string())
        .map_err(anyhow::Error::msg)
}

fn verify_password(password: &str, password_hash: &str) -> bool {
    PasswordHash::new(password_hash).ok().is_some_and(|parsed| {
        Argon2::default()
            .verify_password(password.as_bytes(), &parsed)
            .is_ok()
    })
}

fn parse_role(role: &str) -> Option<Role> {
    match role {
        "admin" => Some(Role::Admin),
        "operator" => Some(Role::Operator),
        "viewer" => Some(Role::Viewer),
        _ => None,
    }
}

async fn login(
    State(state): State<AppState>,
    Json(request): Json<LoginRequest>,
) -> Result<Response, ApiError> {
    let email = request.email.trim().to_ascii_lowercase();
    if email.is_empty() || email.len() > 254 || request.password.len() > 1024 {
        return Ok((StatusCode::UNAUTHORIZED, "invalid credentials").into_response());
    }

    let user = sqlx::query_as::<_, LoginUser>(
        "SELECT id, tenant_id, role, password_hash, is_active \
         FROM auth.users WHERE email = $1",
    )
    .bind(email)
    .fetch_optional(&state.pool)
    .await?;

    let stored_hash = user
        .as_ref()
        .map(|user| user.password_hash.as_str())
        .unwrap_or(&state.dummy_password_hash);
    let password_matches = verify_password(&request.password, stored_hash);
    let Some(user) = user.filter(|user| user.is_active && password_matches) else {
        return Ok((StatusCode::UNAUTHORIZED, "invalid credentials").into_response());
    };
    let Some(role) = parse_role(&user.role) else {
        tracing::error!(user_id = %user.id, "auth user has invalid role");
        return Ok((StatusCode::UNAUTHORIZED, "invalid credentials").into_response());
    };

    let (access_token, expires_at) = state
        .signer
        .issue(user.id, user.tenant_id, role, ACCESS_TOKEN_LIFETIME)
        .map_err(|error| ApiError(anyhow::anyhow!(error)))?;

    Ok((
        StatusCode::OK,
        [(axum::http::header::CACHE_CONTROL, "no-store")],
        Json(LoginResponse {
            access_token,
            token_type: "Bearer",
            expires_at,
            user: LoginUserResponse {
                id: user.id,
                tenant_id: user.tenant_id,
                role,
            },
        }),
    )
        .into_response())
}

async fn health() -> &'static str {
    "auth-service:ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let port = env::var("AUTH_PORT")
        .unwrap_or_else(|_| "4100".to_owned())
        .parse::<u16>()?;
    let database_url = env::var("AUTH_DATABASE_URL").context("AUTH_DATABASE_URL is required")?;
    let tenant_id = env::var("AUTH_BOOTSTRAP_TENANT_ID")
        .context("AUTH_BOOTSTRAP_TENANT_ID is required")?
        .parse::<Uuid>()
        .context("AUTH_BOOTSTRAP_TENANT_ID must be a UUID")?;
    let tenant_name = env::var("AUTH_BOOTSTRAP_TENANT_NAME")
        .unwrap_or_else(|_| "Northstar Supply Co.".to_owned());
    let bootstrap_email = env::var("AUTH_BOOTSTRAP_EMAIL")
        .context("AUTH_BOOTSTRAP_EMAIL is required")?
        .trim()
        .to_ascii_lowercase();
    let bootstrap_password_path = env::var("AUTH_BOOTSTRAP_PASSWORD_FILE")
        .unwrap_or_else(|_| "/run/secrets/bootstrap-password".to_owned());
    let bootstrap_password = fs::read_to_string(bootstrap_password_path)
        .context("read auth bootstrap password secret")?;
    let bootstrap_password = bootstrap_password.trim_end();
    if bootstrap_password.len() < 16 {
        anyhow::bail!("AUTH_BOOTSTRAP_PASSWORD must be at least 16 characters");
    }

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("connect to auth database")?;
    let bootstrap_hash = hash_password(&bootstrap_password)?;
    sqlx::query(
        "INSERT INTO auth.tenants (id, name) VALUES ($1, $2) \
         ON CONFLICT (id) DO NOTHING",
    )
    .bind(tenant_id)
    .bind(tenant_name)
    .execute(&pool)
    .await?;
    sqlx::query(
        "INSERT INTO auth.users (id, tenant_id, email, password_hash, role) \
         VALUES ($1, $2, $3, $4, 'admin') \
         ON CONFLICT (email) DO NOTHING",
    )
    .bind(Uuid::new_v4())
    .bind(tenant_id)
    .bind(bootstrap_email)
    .bind(bootstrap_hash)
    .execute(&pool)
    .await?;

    let issuer = env::var("JWT_ISSUER").unwrap_or_else(|_| "asmdv-auth".to_owned());
    let audience = env::var("JWT_AUDIENCE").unwrap_or_else(|_| "asmdv-platform".to_owned());
    let private_key_path = env::var("JWT_PRIVATE_KEY_PATH")
        .unwrap_or_else(|_| "/run/secrets/jwt-private.pem".to_owned());
    let signer = JwtSigner::from_rsa_pem(&private_key_path, issuer, audience)
        .context("load JWT signing key")?;
    let dummy_password_hash = hash_password("constant dummy password for timing balance")?;

    tracing::info!(tenant_id = %tenant_id, "auth bootstrap account ensured");
    let state = AppState {
        pool,
        signer,
        dummy_password_hash,
    };
    let app = Router::new()
        .route("/health", get(health))
        .route("/api/auth/login", post(login))
        .with_state(state)
        .layer(TraceLayer::new_for_http());
    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "auth service listening");
    axum::serve(listener, app).await?;
    Ok(())
}
