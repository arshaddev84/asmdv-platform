use std::{env, net::SocketAddr};

use anyhow::Context;
use axum::{
    extract::{Extension, Path, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::{get, post},
    Json, Router,
};
use platform_auth::{authenticate, Claims, JwtVerifier};
use serde::{Deserialize, Serialize};
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[derive(Deserialize)]
struct CreateDraftRequest {
    supplier_id: Uuid,
    supplier_name: String,
    lines: Vec<DraftLineRequest>,
}

#[derive(Deserialize, Serialize)]
struct DraftLineRequest {
    product_id: Uuid,
    sku: String,
    description: String,
    quantity: i32,
    unit_cost: f64,
}

#[derive(Serialize)]
struct DraftOrderResponse {
    id: Uuid,
    order_number: String,
    supplier_name: String,
    status: String,
    line_count: usize,
    total: f64,
}

#[derive(FromRow, Serialize)]
struct ReplenishmentRule {
    id: Uuid,
    name: String,
    enabled: bool,
    frequency: String,
    days_of_week: Vec<i16>,
    run_at_local: String,
    business_hours_start: String,
    business_hours_end: String,
    time_zone: String,
    approval_required: bool,
}

#[derive(Deserialize)]
struct UpdateRuleRequest {
    enabled: bool,
    run_at_local: String,
}

#[derive(Debug)]
enum ApiError {
    Internal(anyhow::Error),
    InvalidRequest(&'static str),
    NotFound,
}

impl From<sqlx::Error> for ApiError {
    fn from(error: sqlx::Error) -> Self {
        Self::Internal(error.into())
    }
}

impl IntoResponse for ApiError {
    fn into_response(self) -> Response {
        match self {
            Self::Internal(error) => {
                tracing::error!(error = %error, "procurement request failed");
                (
                    StatusCode::INTERNAL_SERVER_ERROR,
                    "procurement service error",
                )
                    .into_response()
            }
            Self::InvalidRequest(message) => {
                (StatusCode::UNPROCESSABLE_ENTITY, message).into_response()
            }
            Self::NotFound => {
                (StatusCode::NOT_FOUND, "replenishment rule not found").into_response()
            }
        }
    }
}

async fn create_draft(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Json(request): Json<CreateDraftRequest>,
) -> Result<(StatusCode, Json<DraftOrderResponse>), ApiError> {
    if !claims.role.can_create_purchase_orders() {
        return Err(ApiError::InvalidRequest(
            "role is not permitted to create purchase orders",
        ));
    }
    if request.lines.is_empty()
        || request.lines.iter().any(|line| {
            line.quantity <= 0
                || !line.unit_cost.is_finite()
                || line.unit_cost < 0.0
                || line.sku.trim().is_empty()
                || line.description.trim().is_empty()
        })
        || request.supplier_name.trim().is_empty()
    {
        return Err(ApiError::InvalidRequest(
            "A draft order requires a supplier and positive, valid line items",
        ));
    }

    let total: f64 = request
        .lines
        .iter()
        .map(|line| f64::from(line.quantity) * line.unit_cost)
        .sum();
    let lines = serde_json::to_value(&request.lines)
        .context("serialize purchase order lines")
        .map_err(ApiError::Internal)?;
    let row = sqlx::query_as::<_, (Uuid, String)>(
        "INSERT INTO procurement.purchase_orders \
            (tenant_id, supplier_id, supplier_name, status, currency, total_amount, lines) \
         VALUES ($1, $2, $3, 'draft', 'USD', $4, $5) \
         RETURNING id, order_number",
    )
    .bind(claims.tenant_id)
    .bind(request.supplier_id)
    .bind(&request.supplier_name)
    .bind(total)
    .bind(lines)
    .fetch_one(&state.pool)
    .await?;

    Ok((
        StatusCode::CREATED,
        Json(DraftOrderResponse {
            id: row.0,
            order_number: row.1,
            supplier_name: request.supplier_name,
            status: "draft".to_owned(),
            line_count: request.lines.len(),
            total,
        }),
    ))
}

async fn list_drafts(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<DraftOrderResponse>>, ApiError> {
    let rows = sqlx::query_as::<_, (Uuid, String, String, String, i32, f64)>(
        "SELECT id, order_number, supplier_name, status, jsonb_array_length(lines), total_amount::float8 \
         FROM procurement.purchase_orders \
         WHERE tenant_id = $1 AND status IN ('draft', 'pending_approval') \
         ORDER BY created_at DESC LIMIT 50",
    )
    .bind(claims.tenant_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(
        rows.into_iter()
            .map(
                |(id, order_number, supplier_name, status, line_count, total)| DraftOrderResponse {
                    id,
                    order_number,
                    supplier_name,
                    status,
                    line_count: line_count as usize,
                    total,
                },
            )
            .collect(),
    ))
}

async fn list_replenishment_rules(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<ReplenishmentRule>>, ApiError> {
    let rows = sqlx::query_as::<_, ReplenishmentRule>(
        "SELECT id, name, enabled, frequency, days_of_week, \
                TO_CHAR(run_at_local, 'HH24:MI') AS run_at_local, \
                TO_CHAR(business_hours_start, 'HH24:MI') AS business_hours_start, \
                TO_CHAR(business_hours_end, 'HH24:MI') AS business_hours_end, \
                time_zone, approval_required \
         FROM procurement.replenishment_rules \
         WHERE tenant_id = $1 ORDER BY name",
    )
    .bind(claims.tenant_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(Json(rows))
}

fn is_valid_hhmm(value: &str) -> bool {
    let Some((hours, minutes)) = value.split_once(':') else {
        return false;
    };
    if hours.len() != 2 || minutes.len() != 2 {
        return false;
    }
    matches!((hours.parse::<u8>(), minutes.parse::<u8>()), (Ok(h), Ok(m)) if h < 24 && m < 60)
}

async fn update_replenishment_rule(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
    Path(rule_id): Path<Uuid>,
    Json(request): Json<UpdateRuleRequest>,
) -> Result<Json<ReplenishmentRule>, ApiError> {
    if !claims.role.can_manage_replenishment() {
        return Err(ApiError::InvalidRequest(
            "role is not permitted to manage replenishment rules",
        ));
    }
    if !is_valid_hhmm(&request.run_at_local) {
        return Err(ApiError::InvalidRequest(
            "run_at_local must use 24-hour HH:MM format",
        ));
    }

    let rule = sqlx::query_as::<_, ReplenishmentRule>(
        "UPDATE procurement.replenishment_rules \
         SET enabled = $3, run_at_local = $4::TIME, updated_at = NOW() \
         WHERE id = $1 AND tenant_id = $2 \
         RETURNING id, name, enabled, frequency, days_of_week, \
                   TO_CHAR(run_at_local, 'HH24:MI') AS run_at_local, \
                   TO_CHAR(business_hours_start, 'HH24:MI') AS business_hours_start, \
                   TO_CHAR(business_hours_end, 'HH24:MI') AS business_hours_end, \
                   time_zone, approval_required",
    )
    .bind(rule_id)
    .bind(claims.tenant_id)
    .bind(request.enabled)
    .bind(request.run_at_local)
    .fetch_optional(&state.pool)
    .await?
    .ok_or(ApiError::NotFound)?;

    Ok(Json(rule))
}

async fn health() -> &'static str {
    "procurement-service:ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let database_url = env::var("PROCUREMENT_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5433/distributor_procurement".to_owned()
    });
    let port = env::var("PROCUREMENT_PORT")
        .unwrap_or_else(|_| "4102".to_owned())
        .parse::<u16>()
        .context("PROCUREMENT_PORT must be a valid port")?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("connect to procurement database")?;
    let issuer = env::var("JWT_ISSUER").unwrap_or_else(|_| "asmdv-auth".to_owned());
    let audience = env::var("JWT_AUDIENCE").unwrap_or_else(|_| "asmdv-platform".to_owned());
    let public_key_path = env::var("JWT_PUBLIC_KEY_PATH")
        .unwrap_or_else(|_| "/run/secrets/jwt-public.pem".to_owned());
    let verifier = JwtVerifier::from_rsa_pem(&public_key_path, issuer, audience)
        .context("load JWT verification key")?;
    let protected_routes = Router::new()
        .route(
            "/api/v1/purchase-orders/drafts",
            get(list_drafts).post(create_draft),
        )
        .route("/api/v1/replenishment-rules", get(list_replenishment_rules))
        .route(
            "/api/v1/replenishment-rules/:rule_id",
            axum::routing::patch(update_replenishment_rule),
        )
        .route_layer(middleware::from_fn_with_state(verifier, authenticate));
    let app = Router::new()
        .route("/health", get(health))
        .merge(protected_routes)
        .with_state(AppState { pool })
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "procurement service listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::is_valid_hhmm;

    #[test]
    fn accepts_valid_24_hour_times() {
        assert!(is_valid_hhmm("00:00"));
        assert!(is_valid_hhmm("08:30"));
        assert!(is_valid_hhmm("23:59"));
    }

    #[test]
    fn rejects_invalid_time_values() {
        assert!(!is_valid_hhmm("24:00"));
        assert!(!is_valid_hhmm("12:60"));
        assert!(!is_valid_hhmm("8:30"));
        assert!(!is_valid_hhmm("noon"));
    }
}
