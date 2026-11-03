use std::{env, net::SocketAddr};

use anyhow::Context;
use axum::{
    extract::{Extension, State},
    http::StatusCode,
    middleware,
    response::{IntoResponse, Response},
    routing::get,
    Json, Router,
};
use platform_auth::{authenticate, Claims, JwtVerifier};
use serde::Serialize;
use sqlx::{postgres::PgPoolOptions, FromRow, PgPool};
use tower_http::trace::TraceLayer;
use uuid::Uuid;

#[derive(Clone)]
struct AppState {
    pool: PgPool,
}

#[derive(FromRow)]
struct InventoryRow {
    id: Uuid,
    sku: String,
    name: String,
    category: String,
    unit_of_measure: String,
    reorder_point: i32,
    target_stock: i32,
    unit_cost: f64,
    preferred_supplier_id: Uuid,
    supplier_name: String,
    on_hand: i32,
    reserved: i32,
    inbound: i32,
}

#[derive(Serialize)]
struct InventoryItem {
    id: Uuid,
    sku: String,
    name: String,
    category: String,
    unit_of_measure: String,
    reorder_point: i32,
    target_stock: i32,
    unit_cost: f64,
    preferred_supplier_id: Uuid,
    supplier_name: String,
    on_hand: i32,
    reserved: i32,
    inbound: i32,
    available: i32,
    suggested_order_qty: i32,
}

impl From<InventoryRow> for InventoryItem {
    fn from(row: InventoryRow) -> Self {
        let available = (row.on_hand - row.reserved).max(0);
        let suggested_order_qty = calculate_suggested_order_qty(
            available,
            row.inbound,
            row.reorder_point,
            row.target_stock,
        );

        Self {
            id: row.id,
            sku: row.sku,
            name: row.name,
            category: row.category,
            unit_of_measure: row.unit_of_measure,
            reorder_point: row.reorder_point,
            target_stock: row.target_stock,
            unit_cost: row.unit_cost,
            preferred_supplier_id: row.preferred_supplier_id,
            supplier_name: row.supplier_name,
            on_hand: row.on_hand,
            reserved: row.reserved,
            inbound: row.inbound,
            available,
            suggested_order_qty,
        }
    }
}

fn calculate_suggested_order_qty(
    available: i32,
    inbound: i32,
    reorder_point: i32,
    target_stock: i32,
) -> i32 {
    if available.saturating_add(inbound) < reorder_point {
        target_stock
            .saturating_sub(available)
            .saturating_sub(inbound)
            .max(0)
    } else {
        0
    }
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
        tracing::error!(error = %self.0, "inventory request failed");
        (StatusCode::INTERNAL_SERVER_ERROR, "inventory service error").into_response()
    }
}

async fn load_inventory(state: &AppState, tenant_id: Uuid) -> Result<Vec<InventoryItem>, ApiError> {
    let rows = sqlx::query_as::<_, InventoryRow>(
        "SELECT p.id, p.sku, p.name, p.category, p.unit_of_measure, \
                p.reorder_point, p.target_stock, p.unit_cost::float8 AS unit_cost, \
            p.preferred_supplier_id, v.name AS supplier_name, s.on_hand, s.reserved, s.inbound \
         FROM inventory.products p \
         JOIN inventory.stock_levels s ON s.product_id = p.id AND s.tenant_id = p.tenant_id \
         JOIN inventory.suppliers v ON v.id = p.preferred_supplier_id AND v.tenant_id = p.tenant_id \
         WHERE p.tenant_id = $1 AND p.is_active = TRUE \
         ORDER BY p.name",
    )
    .bind(tenant_id)
    .fetch_all(&state.pool)
    .await?;

    Ok(rows.into_iter().map(InventoryItem::from).collect())
}

async fn list_inventory(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<InventoryItem>>, ApiError> {
    Ok(Json(load_inventory(&state, claims.tenant_id).await?))
}

async fn list_reorder_candidates(
    State(state): State<AppState>,
    Extension(claims): Extension<Claims>,
) -> Result<Json<Vec<InventoryItem>>, ApiError> {
    let items = load_inventory(&state, claims.tenant_id)
        .await?
        .into_iter()
        .filter(|item| item.suggested_order_qty > 0)
        .collect();
    Ok(Json(items))
}

async fn health() -> &'static str {
    "inventory-service:ok"
}

#[tokio::main]
async fn main() -> anyhow::Result<()> {
    tracing_subscriber::fmt()
        .with_env_filter(env::var("RUST_LOG").unwrap_or_else(|_| "info".to_owned()))
        .init();

    let database_url = env::var("INVENTORY_DATABASE_URL").unwrap_or_else(|_| {
        "postgres://postgres:postgres@localhost:5432/distributor_inventory".to_owned()
    });
    let port = env::var("INVENTORY_PORT")
        .unwrap_or_else(|_| "4101".to_owned())
        .parse::<u16>()
        .context("INVENTORY_PORT must be a valid port")?;

    let pool = PgPoolOptions::new()
        .max_connections(10)
        .connect(&database_url)
        .await
        .context("connect to inventory database")?;
    let issuer = env::var("JWT_ISSUER").unwrap_or_else(|_| "asmdv-auth".to_owned());
    let audience = env::var("JWT_AUDIENCE").unwrap_or_else(|_| "asmdv-platform".to_owned());
    let public_key_path = env::var("JWT_PUBLIC_KEY_PATH")
        .unwrap_or_else(|_| "/run/secrets/jwt-public.pem".to_owned());
    let verifier = JwtVerifier::from_rsa_pem(&public_key_path, issuer, audience)
        .context("load JWT verification key")?;
    let protected_routes = Router::new()
        .route("/api/v1/inventory", get(list_inventory))
        .route(
            "/api/v1/inventory/reorder-candidates",
            get(list_reorder_candidates),
        )
        .route_layer(middleware::from_fn_with_state(verifier, authenticate));
    let app = Router::new()
        .route("/health", get(health))
        .merge(protected_routes)
        .with_state(AppState { pool })
        .layer(TraceLayer::new_for_http());

    let address = SocketAddr::from(([0, 0, 0, 0], port));
    let listener = tokio::net::TcpListener::bind(address).await?;
    tracing::info!(%address, "inventory service listening");
    axum::serve(listener, app).await?;
    Ok(())
}

#[cfg(test)]
mod tests {
    use super::calculate_suggested_order_qty;

    #[test]
    fn suggests_target_minus_available_and_inbound_when_below_reorder_point() {
        assert_eq!(calculate_suggested_order_qty(34, 0, 60, 180), 146);
        assert_eq!(calculate_suggested_order_qty(23, 10, 50, 120), 87);
    }

    #[test]
    fn does_not_suggest_replenishment_when_stock_and_inbound_reach_reorder_point() {
        assert_eq!(calculate_suggested_order_qty(60, 0, 60, 240), 0);
        assert_eq!(calculate_suggested_order_qty(40, 30, 60, 100), 0);
    }
}
