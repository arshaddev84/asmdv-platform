# Distributor Operations MVP

A small demo platform for distributor inventory and procurement. The web app shows reorder candidates, lets a user create supplier-grouped draft purchase orders, lists open draft/approval-queue orders, and saves weekday replenishment rule settings.

All company, supplier, product, stock, and purchase-order records in this repository are synthetic. No external supplier, ERP, email, or logistics system is contacted.

## Architecture

- `web/`: Next.js App Router dashboard.
- `services/inventory-service/`: Axum API and inventory-owned PostgreSQL schema.
- `services/procurement-service/`: Axum API and procurement-owned PostgreSQL schema.
- `docker-compose.yml`: local demo with separate PostgreSQL databases for each service.

The services communicate through HTTP APIs from the browser in this demo. The inventory API owns stock and supplier catalog data; the procurement API owns purchase orders and replenishment rule settings. No service writes to the other service's database.

## Run Locally

Prerequisite: Docker Compose. From the repository root:

```sh
docker compose --env-file .env.example up --build
```

Open <http://localhost:3000>. The APIs are available at `http://localhost:4101` (inventory) and `http://localhost:4102` (procurement). PostgreSQL is bound to loopback on ports `15432` and `15433` for local inspection, avoiding the default PostgreSQL port.

The compose setup uses fixed local-only demo credentials and seeds each database on its first initialization. To recreate the original seed data, stop the stack and remove its named volumes with `docker compose --env-file .env.example down -v`; this deletes local demo database contents.

For frontend-only development, install dependencies in `web/` with `npm ci`, then run `npm run dev`. The page expects the two APIs at their localhost URLs by default. Local Rust builds use the SQLx native-TLS feature specified by the project and require OpenSSL development headers plus `pkg-config` on Linux.

## Synthetic Data and Schemas

- `services/inventory-service/schema.sql`: inventory tenant, supplier, product, and stock tables.
- `services/procurement-service/schema.sql`: procurement tenant, purchase-order, and replenishment-rule tables.
- `data/synthetic_inventory.sql`: one fictional distributor, three fictional suppliers, and eight SKUs with varied stock/inbound values.
- `data/synthetic_procurement.sql`: a weekday review rule, two sent orders accounting for seeded inbound stock, and sample draft/approval-waiting purchase orders.

The default demo tenant ID is `10000000-0000-4000-8000-000000000001`. The fixtures use reserved `.example` email domains and fixed UUIDs so repeated fresh database initialization is deterministic.

## MVP Behavior

- Available stock is `max(on hand - reserved, 0)`.
- A product becomes a reorder candidate when available stock plus inbound quantity is below its reorder point. Suggested quantity fills toward target stock after accounting for inbound units.
- Selected candidates are grouped by preferred supplier and submitted to procurement as draft purchase orders. Line descriptions, quantities, and unit costs are stored as order snapshots.
- The dashboard lists `draft` and `pending_approval` orders. The seeded pending-approval order is sample data; there is no approval action or PO transmission workflow yet.
- The schedule panel reads and updates the saved weekday rule's enabled state and local review time. It does **not** execute on a timer; no scheduler or background worker is included.
- Service errors are shown in the dashboard. Inventory and procurement can load independently.

Useful endpoints:

| Service | Method and path | Behavior |
| --- | --- | --- |
| Inventory | `GET /health` | Liveness response |
| Inventory | `GET /api/v1/inventory` | Active inventory with available and suggested quantities |
| Inventory | `GET /api/v1/inventory/reorder-candidates` | Items below reorder point |
| Procurement | `GET /health` | Liveness response |
| Procurement | `GET /api/v1/purchase-orders/drafts` | Recent draft and pending-approval POs |
| Procurement | `POST /api/v1/purchase-orders/drafts` | Create one supplier-grouped draft PO |
| Procurement | `GET /api/v1/replenishment-rules` | List tenant rule settings |
| Procurement | `PATCH /api/v1/replenishment-rules/{rule_id}` | Update enabled state and `HH:MM` run time |

## Checks

```sh
cargo fmt --all -- --check
cargo test -p inventory-service
cargo test -p procurement-service
cd web && npm run typecheck && npm run build
```

The Rust tests cover reorder quantity calculations and 24-hour schedule-time validation. Database/API integration tests are not included yet.

## Demo Limitations

This is not production-ready. Authentication and authorization are absent; each service uses a configured demo tenant ID, so the APIs must not be exposed to untrusted users. CORS is permissive for local development. The browser sends supplier and line snapshots to procurement; a production system must authenticate the caller and validate product, supplier, and price data against trusted domain ownership. The schedule is persisted but not run. There is no event broker, transactional outbox, email/call integration, subscription billing, production deployment setup, or audit trail for user actions.

Keep demo credentials and data local. Before production, add identity and tenant authorization, database roles/secrets management, service-to-service contracts, request validation, auditability, background scheduling, reliable messaging, and deployment/observability controls.