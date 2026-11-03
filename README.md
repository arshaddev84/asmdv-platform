# Distributor Operations MVP

A local demo platform for distributor inventory and procurement. The dashboard displays reorder candidates, creates supplier-grouped draft purchase orders, lists draft and pending-approval orders, and edits weekday replenishment-rule settings.

All company, user, supplier, product, stock, and purchase-order data is synthetic. The application does not contact suppliers, ERP systems, email, or logistics services.

## Architecture

- `web/`: Next.js App Router dashboard.
- `services/auth-service/`: Axum login API, bootstrap admin identity, and Argon2id password hashing.
- `services/inventory-service/`: Axum inventory API and inventory-owned PostgreSQL schema.
- `services/procurement-service/`: Axum purchase-order and replenishment-rule API with role-aware writes.
- `crates/platform-auth/`: shared RS256 token signing and verification, bearer authentication middleware, claims, issuer/audience validation, and role permissions.
- `nginx/`: local HTTP ingress and a separate TLS-enabled production profile.
- `docker-compose.yml`: web, three Rust services, three PostgreSQL databases, and Nginx.

The browser sends same-origin `/api/...` requests through Nginx. Only Nginx publishes host ports, bound to loopback by default. The backend services and databases are not published directly. Inventory owns its supplier catalog and stock data; procurement owns purchase orders and replenishment rules. Services do not write to one another's databases.

## Start Locally

Prerequisites: Docker Engine with Docker Compose v2, a POSIX shell, and OpenSSL. From the repository root, run:

```sh
./scripts/dev-up.sh
```

The helper creates missing local keys and a random bootstrap password under the ignored `./secrets/` directory, then runs `docker compose --env-file .env.example up --build -d`. It does not read `.env`.

Open the dashboard at <http://127.0.0.1:8080>. The local Nginx profile serves HTTP; the mapped `8443` port is for use with the production TLS profile and certificates.

To inspect service state and logs:

```sh
docker compose --env-file .env.example ps
docker compose --env-file .env.example logs -f auth-service inventory-service procurement-service nginx web
```

Stop containers while preserving database volumes with:

```sh
docker compose --env-file .env.example down
```

To discard all local database contents and initialize the synthetic fixtures again, run `docker compose --env-file .env.example down -v`. This permanently deletes the Compose database volumes.

## Demo Login

The auth bootstrap creates this admin user if it does not already exist:

| Field | Value |
| --- | --- |
| Email | `admin@northstar.example` |
| Role | `admin` |
| Password | Randomly generated and stored in `./secrets/dev-bootstrap-password` |

Read the local password when signing in; the helper does not print it:

```sh
cat ./secrets/dev-bootstrap-password
```

The password is not a fixed value in the repository. Keep the ignored password file alongside the auth database volume. Bootstrap uses `ON CONFLICT DO NOTHING`, so deleting or regenerating the file does not change the password hash for an account already present in the database. For a fresh demo identity, use a fresh auth database volume; removing volumes also deletes all other local data.

The login endpoint is `POST /api/auth/login` with JSON fields `email` and `password`. On success, the response contains an RS256 bearer access token and expiry. Tokens last 15 minutes. The dashboard keeps the token in memory, sends it in the `Authorization: Bearer` header, and asks the user to sign in again when it expires or the page is reloaded.

The local database values in `.env.example` (`demo` / `demo-local-only`) are for this demo only. Do not reuse them outside a local environment or expose the databases.

## Authentication and Roles

Protected APIs require a valid RS256 JWT. Services check the configured issuer and audience and validate expiration. Claims include user ID, tenant ID, role, issue/expiry time, and token ID. Inventory and procurement queries and mutations use the authenticated tenant ID rather than a tenant ID supplied by the browser.

| Role | Read APIs | Create draft purchase orders | Manage replenishment rules |
| --- | --- | --- | --- |
| `admin` | Yes | Yes | Yes |
| `operator` | Yes | Yes | No |
| `viewer` | Yes | No | No |

The UI hides actions unavailable to the current role, and procurement enforces the same permissions on the server. Missing or invalid bearer tokens receive `401`; procurement returns `422` when a role-disallowed mutation is attempted.

## Nginx and TLS

The default `nginx/local.conf` profile uses HTTP on `127.0.0.1:8080`, applies security headers, and rate-limits login requests. It routes the dashboard and auth, inventory, and procurement endpoints to their internal services. `/healthz` is the gateway health endpoint.

The optional `nginx/production.conf` profile redirects HTTP to HTTPS and enables TLS 1.2/1.3, HSTS, security headers, and a stricter login rate limit. It expects certificate files at `./secrets/fullchain.pem` and `./secrets/privkey.pem`. Provide valid certificates before selecting this profile; the repository does not issue certificates. For example, with those files in place:

```sh
NGINX_CONFIG_FILE=./nginx/production.conf docker compose --env-file .env.example up -d nginx
```

The production Nginx profile is a configuration example, not a complete production deployment or readiness guarantee.

## API Endpoints

Business API paths below are served through Nginx at `http://127.0.0.1:8080` in the local profile and require a bearer token except for login. Service `/health` endpoints are internal to the Compose network; Nginx exposes `/healthz` for the gateway health check.

| Service | Method and path | Behavior |
| --- | --- | --- |
| Gateway | `GET /healthz` | Nginx liveness response |
| Auth | `GET /health` | Auth service liveness response; internal to Compose network |
| Auth | `POST /api/auth/login` | Verify email/password and issue an access token |
| Inventory | `GET /health` | Inventory service liveness response; internal to Compose network |
| Inventory | `GET /api/v1/inventory` | Active inventory with available and suggested quantities |
| Inventory | `GET /api/v1/inventory/reorder-candidates` | Items below their reorder point |
| Procurement | `GET /health` | Procurement service liveness response; internal to Compose network |
| Procurement | `GET /api/v1/purchase-orders/drafts` | Recent draft and pending-approval purchase orders |
| Procurement | `POST /api/v1/purchase-orders/drafts` | Create supplier-grouped draft purchase orders; admin/operator only |
| Procurement | `GET /api/v1/replenishment-rules` | List tenant rule settings |
| Procurement | `PATCH /api/v1/replenishment-rules/{rule_id}` | Update enabled state and `HH:MM` review time; admin only |

## Synthetic Data and Behavior

- `services/auth-service/schema.sql`: auth tenants and users.
- `services/inventory-service/schema.sql`: inventory tenants, suppliers, products, and stock.
- `services/procurement-service/schema.sql`: purchase orders, order lines, and replenishment rules.
- `data/synthetic_inventory.sql`: one fictional distributor, three fictional suppliers, and eight SKUs with varied stock and inbound values.
- `data/synthetic_procurement.sql`: a weekday review rule, two sent orders that account for seeded inbound stock, and example draft/pending-approval orders.

The default demo tenant ID is `10000000-0000-4000-8000-000000000001`. Fixtures use reserved `.example` email domains and fixed UUIDs for deterministic fresh database initialization. Schema and seed SQL are applied only when a database volume is initialized for the first time.

- Available stock is `max(on hand - reserved, 0)`.
- An item is a reorder candidate when available stock plus inbound quantity is below its reorder point. Suggested quantity fills toward target stock after accounting for inbound units.
- Selected candidates are grouped by preferred supplier and submitted as draft purchase orders. Line descriptions, quantities, and unit costs are stored as order snapshots.
- The dashboard lists `draft` and `pending_approval` orders. The seeded pending-approval order is sample data; there is no approval action or purchase-order transmission workflow.
- Replenishment settings persist the enabled state and weekday/local review time. They are not executed on a timer; there is no scheduler or background worker.
- Inventory and procurement data can load independently, and service errors are shown in the dashboard.

## Development and Validation

Rust workspace checks (host builds may require OpenSSL development headers and `pkg-config` because SQLx uses native TLS):

```sh
cargo fmt --all -- --check
cargo test --workspace
```

Frontend checks:

```sh
cd web
npm ci
npm run typecheck
npm run build
```

Full Compose validation and local startup:

```sh
docker compose --env-file .env.example config -q
docker compose --env-file .env.example build
./scripts/dev-up.sh
docker compose --env-file .env.example exec -T nginx nginx -t
curl --fail http://127.0.0.1:8080/healthz
```

The current Rust unit tests cover role permissions, reorder quantity calculations, and 24-hour review-time validation. Database/API integration tests are not included yet.

## Limitations

This is not production-ready. There is no refresh-token flow, token revocation, account recovery, comprehensive audit trail, production secret rotation, database role separation, backup strategy, or production observability stack. Login rate limiting is configured in Nginx; broader application rate limiting is not implemented. Replenishment rules are stored but not scheduled. There is no event broker, transactional outbox, external supplier/ERP integration, or email/call workflow.

Keep demo credentials, keys, and data local. Before production, add identity lifecycle and revocation controls, database least-privilege roles, secret rotation, auditability, real background scheduling, reliable messaging, backups, and operational monitoring.