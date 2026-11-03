CREATE SCHEMA IF NOT EXISTS inventory;

CREATE TABLE inventory.tenants (
    id UUID PRIMARY KEY,
    name TEXT NOT NULL,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW()
);

CREATE TABLE inventory.suppliers (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES inventory.tenants(id),
    name TEXT NOT NULL,
    contact_email TEXT NOT NULL,
    lead_time_days INTEGER NOT NULL CHECK (lead_time_days >= 0),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, name)
);

CREATE TABLE inventory.products (
    id UUID PRIMARY KEY,
    tenant_id UUID NOT NULL REFERENCES inventory.tenants(id),
    preferred_supplier_id UUID NOT NULL,
    sku TEXT NOT NULL,
    name TEXT NOT NULL,
    category TEXT NOT NULL,
    unit_of_measure TEXT NOT NULL,
    reorder_point INTEGER NOT NULL CHECK (reorder_point >= 0),
    target_stock INTEGER NOT NULL CHECK (target_stock >= reorder_point),
    unit_cost NUMERIC(12, 2) NOT NULL CHECK (unit_cost >= 0),
    is_active BOOLEAN NOT NULL DEFAULT TRUE,
    created_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    FOREIGN KEY (tenant_id, preferred_supplier_id)
        REFERENCES inventory.suppliers(tenant_id, id),
    UNIQUE (tenant_id, id),
    UNIQUE (tenant_id, sku)
);

CREATE TABLE inventory.stock_levels (
    tenant_id UUID NOT NULL,
    product_id UUID NOT NULL,
    on_hand INTEGER NOT NULL CHECK (on_hand >= 0),
    reserved INTEGER NOT NULL CHECK (reserved >= 0 AND reserved <= on_hand),
    inbound INTEGER NOT NULL DEFAULT 0 CHECK (inbound >= 0),
    updated_at TIMESTAMPTZ NOT NULL DEFAULT NOW(),
    PRIMARY KEY (tenant_id, product_id),
    FOREIGN KEY (tenant_id, product_id)
        REFERENCES inventory.products(tenant_id, id)
);

CREATE INDEX products_tenant_active_idx
    ON inventory.products (tenant_id, is_active, name);