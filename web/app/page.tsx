"use client";

import {
  AlertTriangle,
  ArrowUpRight,
  CalendarClock,
  Check,
  CircleDollarSign,
  Clock3,
  FilePlus2,
  LoaderCircle,
  PackageCheck,
  RefreshCw,
} from "lucide-react";
import { FormEvent, useEffect, useMemo, useState } from "react";

const inventoryApi = process.env.NEXT_PUBLIC_INVENTORY_API_URL ?? "http://localhost:4101";
const procurementApi = process.env.NEXT_PUBLIC_PROCUREMENT_API_URL ?? "http://localhost:4102";

type InventoryItem = {
  id: string;
  sku: string;
  name: string;
  category: string;
  unit_of_measure: string;
  reorder_point: number;
  target_stock: number;
  unit_cost: number;
  preferred_supplier_id: string;
  supplier_name: string;
  on_hand: number;
  reserved: number;
  inbound: number;
  available: number;
  suggested_order_qty: number;
};

type PurchaseOrder = {
  id: string;
  order_number: string;
  supplier_name: string;
  status: string;
  line_count: number;
  total: number;
};

type ReplenishmentRule = {
  id: string;
  name: string;
  enabled: boolean;
  frequency: string;
  days_of_week: number[];
  run_at_local: string;
  business_hours_start: string;
  business_hours_end: string;
  time_zone: string;
  approval_required: boolean;
};

type DraftLine = {
  product_id: string;
  sku: string;
  description: string;
  quantity: number;
  unit_cost: number;
};

async function getJson<T>(url: string): Promise<T> {
  const response = await fetch(url, { cache: "no-store" });
  if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
  return response.json() as Promise<T>;
}

function money(value: number) {
  return new Intl.NumberFormat("en-US", { style: "currency", currency: "USD" }).format(value);
}

function weekdayLabel(days: number[]) {
  const names = ["Mon", "Tue", "Wed", "Thu", "Fri", "Sat", "Sun"];
  return days.map((day) => names[day - 1]).filter(Boolean).join(", ");
}

export default function Home() {
  const [inventory, setInventory] = useState<InventoryItem[]>([]);
  const [orders, setOrders] = useState<PurchaseOrder[]>([]);
  const [rules, setRules] = useState<ReplenishmentRule[]>([]);
  const [selectedIds, setSelectedIds] = useState<string[]>([]);
  const [loading, setLoading] = useState(true);
  const [creating, setCreating] = useState(false);
  const [savingRule, setSavingRule] = useState(false);
  const [scheduleTime, setScheduleTime] = useState("08:30");
  const [ruleEnabled, setRuleEnabled] = useState(true);
  const [refreshKey, setRefreshKey] = useState(0);
  const [inventoryError, setInventoryError] = useState("");
  const [procurementError, setProcurementError] = useState("");
  const [ruleError, setRuleError] = useState("");
  const [notice, setNotice] = useState("");

  useEffect(() => {
    let active = true;
    setLoading(true);

    Promise.allSettled([
      getJson<InventoryItem[]>(`${inventoryApi}/api/v1/inventory/reorder-candidates`),
      getJson<PurchaseOrder[]>(`${procurementApi}/api/v1/purchase-orders/drafts`),
      getJson<ReplenishmentRule[]>(`${procurementApi}/api/v1/replenishment-rules`),
    ]).then(([inventoryResult, ordersResult, rulesResult]) => {
      if (!active) return;
      if (inventoryResult.status === "fulfilled") {
        setInventory(inventoryResult.value);
        setSelectedIds(inventoryResult.value.map((item) => item.id));
        setInventoryError("");
      } else {
        setInventoryError("Inventory service unavailable. Check the service and database.");
      }
      if (ordersResult.status === "fulfilled") {
        setOrders(ordersResult.value);
        setProcurementError("");
      } else {
        setProcurementError("Procurement service unavailable. Check the service and database.");
      }
      if (rulesResult.status === "fulfilled") {
        setRules(rulesResult.value);
        if (rulesResult.value[0]) {
          setScheduleTime(rulesResult.value[0].run_at_local);
          setRuleEnabled(rulesResult.value[0].enabled);
        }
        setRuleError("");
      } else {
        setRuleError("Schedule settings could not be loaded.");
      }
      setLoading(false);
    });

    return () => {
      active = false;
    };
  }, [refreshKey]);

  const selectedItems = useMemo(
    () => inventory.filter((item) => selectedIds.includes(item.id)),
    [inventory, selectedIds],
  );
  const suggestedSpend = selectedItems.reduce(
    (total, item) => total + item.suggested_order_qty * item.unit_cost,
    0,
  );
  const pendingApprovalCount = orders.filter((order) => order.status === "pending_approval").length;

  function toggleItem(id: string) {
    setSelectedIds((current) =>
      current.includes(id) ? current.filter((value) => value !== id) : [...current, id],
    );
  }

  async function createDraftOrders() {
    const groups = new Map<string, { supplier_id: string; supplier_name: string; lines: DraftLine[] }>();
    for (const item of selectedItems) {
      const group = groups.get(item.preferred_supplier_id) ?? {
        supplier_id: item.preferred_supplier_id,
        supplier_name: item.supplier_name,
        lines: [],
      };
      group.lines.push({
        product_id: item.id,
        sku: item.sku,
        description: item.name,
        quantity: item.suggested_order_qty,
        unit_cost: item.unit_cost,
      });
      groups.set(item.preferred_supplier_id, group);
    }

    setCreating(true);
    setNotice("");
    const results = await Promise.allSettled(
      [...groups.values()].map(async (group) => {
        const response = await fetch(`${procurementApi}/api/v1/purchase-orders/drafts`, {
          method: "POST",
          headers: { "Content-Type": "application/json" },
          body: JSON.stringify(group),
        });
        if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
        return response.json() as Promise<PurchaseOrder>;
      }),
    );
    const created = results.filter((result) => result.status === "fulfilled").length;
    setCreating(false);
    if (created > 0) setNotice(`${created} draft purchase order${created === 1 ? "" : "s"} created.`);
    if (created < results.length) {
      setProcurementError("Some draft orders could not be created. Refresh to see saved orders.");
    }
    setRefreshKey((key) => key + 1);
  }

  async function saveRule(event: FormEvent<HTMLFormElement>) {
    event.preventDefault();
    const rule = rules[0];
    if (!rule) return;
    setSavingRule(true);
    setRuleError("");
    try {
      const response = await fetch(`${procurementApi}/api/v1/replenishment-rules/${rule.id}`, {
        method: "PATCH",
        headers: { "Content-Type": "application/json" },
        body: JSON.stringify({ enabled: ruleEnabled, run_at_local: scheduleTime }),
      });
      if (!response.ok) throw new Error(`${response.status} ${response.statusText}`);
      const updated = (await response.json()) as ReplenishmentRule;
      setRules([updated]);
      setNotice("Schedule settings saved. Automatic execution is not enabled in this demo.");
    } catch {
      setRuleError("Schedule settings could not be saved.");
    } finally {
      setSavingRule(false);
    }
  }

  return (
    <main className="app-shell">
      <header className="topbar">
        <a className="brand" href="#overview" aria-label="Northstar overview">
          <span className="brand-mark"><PackageCheck size={19} strokeWidth={2.2} /></span>
          <span>northstar<span className="brand-period">.</span></span>
        </a>
        <div className="topbar-right">
          <span className="tenant-name">Northstar Supply Co.</span>
          <span className="demo-chip">Synthetic demo</span>
          <button
            className="icon-button"
            type="button"
            aria-label="Refresh data"
            title="Refresh data"
            onClick={() => setRefreshKey((key) => key + 1)}
          >
            <RefreshCw size={17} />
          </button>
        </div>
      </header>

      <div className="workspace" id="overview">
        <div className="page-heading">
          <div>
            <p className="eyebrow">OPERATIONS / PROCUREMENT</p>
            <h1>Procurement overview</h1>
            <p className="subheading">Inventory signals and purchase orders in one place.</p>
          </div>
          <div className="heading-meta"><span className="live-dot" />Demo workspace</div>
        </div>

        <section className="metrics" aria-label="Procurement summary">
          <div className="metric">
            <div className="metric-label"><AlertTriangle size={15} /> Reorder candidates</div>
            <div className="metric-value">{loading ? "--" : inventory.length}</div>
            <div className="metric-note">Below configured stock thresholds</div>
          </div>
          <div className="metric">
            <div className="metric-label"><CircleDollarSign size={15} /> Proposed spend</div>
            <div className="metric-value">{money(suggestedSpend)}</div>
            <div className="metric-note">Selected items at current unit cost</div>
          </div>
          <div className="metric metric-accent">
            <div className="metric-label"><Clock3 size={15} /> Awaiting approval</div>
            <div className="metric-value">{loading ? "--" : pendingApprovalCount}</div>
            <div className="metric-note">Draft orders requiring review</div>
          </div>
        </section>

        {notice && <div className="notice" role="status"><Check size={16} />{notice}</div>}

        <div className="content-grid">
          <section className="panel inventory-panel">
            <div className="panel-heading">
              <div>
                <p className="eyebrow">STOCK CONTROL</p>
                <h2>Reorder candidates</h2>
              </div>
              <button
                className="primary-button"
                type="button"
                onClick={createDraftOrders}
                disabled={creating || selectedItems.length === 0}
              >
                {creating ? <LoaderCircle className="spin" size={16} /> : <FilePlus2 size={16} />}
                Create draft POs
                <ArrowUpRight size={15} />
              </button>
            </div>
            {inventoryError && <p className="error-message">{inventoryError}</p>}
            <div className="table-wrap">
              <table>
                <thead>
                  <tr>
                    <th className="select-column"><span className="sr-only">Select</span></th>
                    <th>Item</th>
                    <th>Supplier</th>
                    <th>Available</th>
                    <th>Inbound</th>
                    <th>Suggested</th>
                    <th className="amount-column">Est. cost</th>
                  </tr>
                </thead>
                <tbody>
                  {loading && inventory.length === 0 ? (
                    <tr><td colSpan={7} className="empty-cell">Loading inventory…</td></tr>
                  ) : inventoryError ? (
                    <tr><td colSpan={7} className="empty-cell">No inventory connection</td></tr>
                  ) : inventory.length === 0 ? (
                    <tr><td colSpan={7} className="empty-cell">No reorder candidates right now.</td></tr>
                  ) : inventory.map((item) => (
                    <tr key={item.id}>
                      <td className="select-column">
                        <input
                          type="checkbox"
                          checked={selectedIds.includes(item.id)}
                          onChange={() => toggleItem(item.id)}
                          aria-label={`Select ${item.name}`}
                        />
                      </td>
                      <td>
                        <span className="item-name">{item.name}</span>
                        <span className="item-subline">{item.sku} <span>·</span> {item.category}</span>
                      </td>
                      <td className="supplier-cell">{item.supplier_name}</td>
                      <td><span className="stock-low">{item.available}</span><span className="unit"> {item.unit_of_measure}</span></td>
                      <td>{item.inbound} <span className="unit">{item.unit_of_measure}</span></td>
                      <td className="qty-cell">{item.suggested_order_qty} <span className="unit">{item.unit_of_measure}</span></td>
                      <td className="amount-column">{money(item.suggested_order_qty * item.unit_cost)}</td>
                    </tr>
                  ))}
                </tbody>
              </table>
            </div>
            <div className="table-footer">
              <span>{selectedItems.length} of {inventory.length} selected</span>
              <span>Suggestions account for reserved and inbound stock</span>
            </div>
          </section>

          <aside className="side-column">
            <section className="schedule-panel">
              <div className="schedule-title">
                <div className="schedule-icon"><CalendarClock size={18} /></div>
                <div><p className="eyebrow">RULE SETTINGS</p><h2>Replenishment review</h2></div>
              </div>
              {ruleError && <p className="error-message">{ruleError}</p>}
              {rules[0] ? (
                <form onSubmit={saveRule}>
                  <p className="rule-name">{rules[0].name}</p>
                  <div className="rule-detail">
                    <span>Run days</span><strong>{weekdayLabel(rules[0].days_of_week)}</strong>
                  </div>
                  <div className="rule-detail">
                    <span>Business hours</span>
                    <strong>{rules[0].business_hours_start}–{rules[0].business_hours_end}</strong>
                  </div>
                  <div className="rule-detail">
                    <span>Time zone</span><strong>{rules[0].time_zone}</strong>
                  </div>
                  <label className="time-field">
                    <span>Review time</span>
                    <input type="time" value={scheduleTime} onChange={(event) => setScheduleTime(event.target.value)} />
                  </label>
                  <label className="toggle-row">
                    <span><strong>Rule enabled</strong><small>Approval required before a PO is released</small></span>
                    <input type="checkbox" checked={ruleEnabled} onChange={(event) => setRuleEnabled(event.target.checked)} />
                  </label>
                  <button className="secondary-button" type="submit" disabled={savingRule}>
                    {savingRule ? <LoaderCircle className="spin" size={15} /> : <Check size={15} />}
                    Save schedule settings
                  </button>
                  <p className="schedule-footnote">Settings are saved; scheduled execution is not active in this MVP.</p>
                </form>
              ) : loading ? (
                <p className="muted-copy">Loading schedule…</p>
              ) : (
                <p className="muted-copy">No replenishment rule is configured.</p>
              )}
            </section>
            <section className="tip-panel">
              <div className="tip-icon"><PackageCheck size={17} /></div>
              <div>
                <h3>Approval-first workflow</h3>
                <p>New purchase orders stay in draft until a team member reviews them.</p>
              </div>
            </section>
          </aside>
        </div>

        <section className="panel orders-panel">
          <div className="panel-heading">
            <div><p className="eyebrow">PROCUREMENT QUEUE</p><h2>Draft purchase orders</h2></div>
            <span className="queue-count">{orders.length} open</span>
          </div>
          {procurementError && <p className="error-message">{procurementError}</p>}
          <div className="table-wrap">
            <table className="orders-table">
              <thead><tr><th>PO number</th><th>Supplier</th><th>Lines</th><th>Status</th><th className="amount-column">Total</th></tr></thead>
              <tbody>
                {orders.length === 0 ? (
                  <tr><td colSpan={5} className="empty-cell">No open draft purchase orders.</td></tr>
                ) : orders.map((order) => (
                  <tr key={order.id}>
                    <td className="po-number">{order.order_number}</td>
                    <td>{order.supplier_name}</td>
                    <td>{order.line_count}</td>
                    <td><span className={`status-pill status-${order.status}`}>{order.status.replaceAll("_", " ")}</span></td>
                    <td className="amount-column">{money(order.total)}</td>
                  </tr>
                ))}
              </tbody>
            </table>
          </div>
        </section>

        <footer className="page-footer">
          <span><span className="footer-mark">N</span> Northstar Supply Co.</span>
          <span>Sample company and transaction data are synthetic.</span>
        </footer>
      </div>
    </main>
  );
}