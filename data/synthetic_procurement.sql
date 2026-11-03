-- Fictional starting rule and open draft purchase orders for the demo tenant.
INSERT INTO procurement.tenants (id, name)
VALUES ('10000000-0000-4000-8000-000000000001', 'Northstar Supply Co.')
ON CONFLICT (id) DO NOTHING;

INSERT INTO procurement.replenishment_rules (
    id, tenant_id, name, frequency, days_of_week, run_at_local,
    business_hours_start, business_hours_end, time_zone, approval_required
)
VALUES (
    '40000000-0000-4000-8000-000000000001',
    '10000000-0000-4000-8000-000000000001',
    'Weekday morning replenishment review',
    'weekly', ARRAY[1, 2, 3, 4, 5]::SMALLINT[], '08:30',
    '08:00', '17:00', 'America/Chicago', TRUE
)
ON CONFLICT (id) DO NOTHING;

INSERT INTO procurement.purchase_orders (
    id, tenant_id, order_number, supplier_id, supplier_name, status, currency, total_amount, lines
)
VALUES
    (
        '50000000-0000-4000-8000-000000000003',
        '10000000-0000-4000-8000-000000000001',
        'PO-2026-00100',
        '20000000-0000-4000-8000-000000000001',
        'Great Lakes Packaging', 'sent', 'USD', 127.50,
        '[{"product_id":"30000000-0000-4000-8000-000000000002","sku":"PKG-WR-18","description":"Stretch wrap, 18 in x 1500 ft","quantity":10,"unit_cost":12.75}]'::JSONB
    ),
    (
        '50000000-0000-4000-8000-000000000004',
        '10000000-0000-4000-8000-000000000001',
        'PO-2026-00099',
        '20000000-0000-4000-8000-000000000003',
        'Metro Safety & Janitorial', 'sent', 'USD', 479.40,
        '[{"product_id":"30000000-0000-4000-8000-000000000006","sku":"CLN-MP-1G","description":"Neutral floor cleaner, 1 gal","quantity":12,"unit_cost":39.95}]'::JSONB
    ),
    (
        '50000000-0000-4000-8000-000000000001',
        '10000000-0000-4000-8000-000000000001',
        'PO-2026-00101',
        '20000000-0000-4000-8000-000000000001',
        'Great Lakes Packaging', 'draft', 'USD', 1224.00,
        '[{"product_id":"30000000-0000-4000-8000-000000000002","sku":"PKG-WR-18","description":"Stretch wrap, 18 in x 1500 ft","quantity":96,"unit_cost":12.75}]'::JSONB
    ),
    (
        '50000000-0000-4000-8000-000000000002',
        '10000000-0000-4000-8000-000000000001',
        'PO-2026-00102',
        '20000000-0000-4000-8000-000000000003',
        'Metro Safety & Janitorial', 'pending_approval', 'USD', 1176.00,
        '[{"product_id":"30000000-0000-4000-8000-000000000001","sku":"SAFE-NG-100","description":"Nitrile gloves, large (100 ct)","quantity":140,"unit_cost":8.40}]'::JSONB
    )
ON CONFLICT (id) DO NOTHING;

SELECT SETVAL('procurement.purchase_order_number_seq', 102, TRUE);