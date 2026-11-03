CREATE SCHEMA IF NOT EXISTS procurement;

CREATE TABLE procurement.tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE SEQUENCE procurement.purchase_order_number_seq START 1;

CREATE TABLE procurement.purchase_orders (
    id UUID PRIMARY KEY DEFAULT gen_random_uuid(),
    tenant_id UUID NOT NULL REFERENCES procurement.tenants(id),
    order_number TEXT NOT NULL UNIQUE DEFAULT (
        'PO-' || TO_CHAR(CURRENT_DATE, 'YYYY') || '-' ||
        LPAD(NEXTVAL('procurement.purchase_order_number_seq')::TEXT, 5, '0')
    ),
    supplier_id UUID NOT NULL,
    supplier_name TEXT NOT NULL,
    status TEXT NOT NULL DEFAULT 'draft'
        CHECK (status IN ('draft', 'pending_approval', 'approved', 'sent', 'received', 'cancelled')),
    currency CHAR(3) NOT NULL DEFAULT 'USD',
    total_amount NUMERIC(14, 2) NOT NULL CHECK (total_amount >= 0),
    lines JSONB NOT NULL CHECK (
        JSONB_TYPEOF(lines) = 'array' AND JSONB_ARRAY_LENGTH(lines) > 0
    ),
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE INDEX purchase_orders_tenant_status_created_idx
    ON procurement.purchase_orders (tenant_id, status, created_at DESC);

CREATE TABLE procurement.replenishment_rules (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES procurement.tenants(id),
    name TEXT NOT NULL,
    enabled BOOLEAN NOT NULL DEFAULT TRUE,
    frequency TEXT NOT NULL CHECK (frequency IN ('daily', 'weekly')),
    days_of_week SMALLINT[] NOT NULL DEFAULT ARRAY[]::SMALLINT[]
        CHECK (days_of_week <@ ARRAY[1, 2, 3, 4, 5, 6, 7]::SMALLINT[]),
    run_at_local TIME NOT NULL,
    business_hours_start TIME NOT NULL,
    business_hours_end TIME NOT NULL,
    time_zone TEXT NOT NULL,
    approval_required BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    CHECK (business_hours_start < business_hours_end),
    CHECK (frequency <> 'weekly' OR CARDINALITY(days_of_week) > 0)
);

CREATE INDEX replenishment_rules_tenant_idx
    ON procurement.replenishment_rules (tenant_id, enabled);