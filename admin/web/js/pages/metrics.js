// Metrics (Kenny 2026-09-30, decision "Charts and Traffic → Metrics"):
// System (replace-grafana) and Traffic (replace-goaccess) as two tabs of
// one page, sharing the window (1h/6h/24h/7d/30d) so switching tabs keeps
// it — the window lives in the address, like every other filter here
// (feat-overview-8), so the tab switch is a navigation, not a new control.

import { panelEl } from "../charts.js";
import { errorBox, h, fetchJson } from "../dom.js";
import { formatDateTime } from "../format.js";
import { current, subscribe } from "../store.js";
import { choice, setParams } from "../urlstate.js";

const RANGES = /** @type {const} */ (["1h", "6h", "24h", "7d", "30d"]);
const TABS = /** @type {const} */ ([
  { tab: "system", label: "System" },
  { tab: "traffic", label: "Traffic" },
]);

/**
 * A two-column table of the busiest names in the window, with the
 * one-sentence description rule 8 asks of every section. fix-221: `t.error`
 * is always a short, already-stripped reason by the time it reaches here
 * (never an upstream's raw HTML page) — see `Loki::metric_now`'s
 * `upstream_reason`.
 * @param {string} caption
 * @param {string} desc
 * @param {string} what
 * @param {{rows: [string, number][], error?: string}} t
 */
function topTable(caption, desc, what, t) {
  if (t.error) return errorBox({ what: caption, why: t.error, fix: "" });
  return h(
    "table",
    { class: "kp-table" },
    h(
      "caption",
      null,
      caption,
      h("p", { class: "section-head__desc measured" }, desc),
    ),
    h(
      "thead",
      null,
      h("tr", null, h("th", null, what), h("th", null, "Requests")),
    ),
    h(
      "tbody",
      null,
      ...t.rows.map(([k, n]) =>
        h(
          "tr",
          null,
          h("td", null, k || "—"),
          h("td", null, String(Math.round(n))),
        ),
      ),
    ),
  );
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  const params = new URLSearchParams(location.search);
  const tab = choice(
    params,
    "tab",
    TABS.map((t) => t.tab),
    "system",
  );
  const range = choice(params, "range", RANGES, "24h");
  const stack = params.get("stack") ?? "";

  const tabs = h(
    "div",
    { class: "chart-ranges", role: "tablist", "aria-label": "Metrics" },
    ...TABS.map((t) => {
      const b = h(
        "button",
        {
          type: "button",
          role: "tab",
          "aria-selected": t.tab === tab ? "true" : "false",
          class: `kp-button${t.tab === tab ? " kp-button--primary" : ""}`,
        },
        t.label,
      );
      b.addEventListener("click", () =>
        ctx.navigate(`/charts${setParams(location.search, { tab: t.tab })}`),
      );
      return b;
    }),
  );

  const pick = h(
    "select",
    tab === "system"
      ? { class: "kp-field__input", "aria-label": "Which charts" }
      : { class: "kp-field__input", "aria-label": "Which charts", hidden: "" },
  );
  const fillStacks = () => {
    const names = (current().fleet?.stacks ?? []).map((s) => s.name).sort();
    pick.replaceChildren(
      h("option", { value: "" }, "The host"),
      ...names.map((n) => h("option", { value: n }, `Stack ${n}`)),
    );
    pick.value = stack;
  };
  const unsub = tab === "system" ? subscribe(fillStacks) : () => {};
  if (tab === "system") fillStacks();
  pick.addEventListener("change", () =>
    ctx.navigate(`/charts${setParams(location.search, { stack: pick.value })}`),
  );

  const ranges = h(
    "div",
    { class: "chart-ranges", role: "group", "aria-label": "Window" },
    ...RANGES.map((r) => {
      const b = h(
        "button",
        {
          type: "button",
          class: `kp-button${r === range ? " kp-button--primary" : ""}`,
          "aria-pressed": r === range ? "true" : "false",
        },
        r,
      );
      b.addEventListener("click", () =>
        ctx.navigate(`/charts${setParams(location.search, { range: r })}`),
      );
      return b;
    }),
  );

  const status = h("p", { class: "measured", role: "status" }, "Reading…");
  const grid = h("div", { class: "chart-grid" });
  const tables = h("div", { class: "chart-grid" });
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Metrics"),
      tabs,
      pick,
      ranges,
    ),
    h(
      "p",
      { class: "section-head__desc measured" },
      "System and traffic charts for the host and the fleet, read from Prometheus.",
    ),
    status,
    grid,
    tables,
  );

  const abort = new AbortController();
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let timer;

  if (tab === "system") {
    status.textContent = "Reading the charts…";
    (async () => {
      const q = setParams("", { stack: stack || null, range });
      const r = await fetchJson(`/data/charts${q}`, "the charts", abort.signal);
      if (!r.ok) {
        status.textContent = "";
        grid.replaceChildren(errorBox(r.error));
        return;
      }
      status.textContent = `${stack ? `Stack ${stack}` : "The host"}, ${formatDateTime(r.body.from)} to ${formatDateTime(r.body.to)}`;
      grid.replaceChildren(
        ...r.body.panels.map((/** @type {any} */ p) =>
          panelEl(p, r.body.from, r.body.to),
        ),
      );
    })().catch(() => {});
  } else {
    status.textContent = "Reading the access log…";
    const read = async () => {
      const r = await fetchJson(
        `/data/traffic${setParams("", { range })}`,
        "the traffic",
        abort.signal,
      );
      if (!r.ok) {
        status.textContent = "";
        grid.replaceChildren(errorBox(r.error));
        tables.replaceChildren();
        return;
      }
      const b = r.body;
      status.textContent = `${formatDateTime(b.from)} to ${formatDateTime(b.to)} · reads again every 30 s`;
      grid.replaceChildren(
        ...b.panels.map((/** @type {any} */ p) => panelEl(p, b.from, b.to)),
      );
      tables.replaceChildren(
        topTable(
          "Busiest hostnames",
          "The hostnames with the most requests in this window.",
          "Hostname",
          b.hosts,
        ),
        topTable(
          "Busiest client addresses",
          "The client addresses that made the most requests in this window.",
          "Client",
          b.clients,
        ),
      );
    };
    read().catch(() => {});
    timer = setInterval(() => read().catch(() => {}), 30_000);
  }

  return () => {
    abort.abort();
    unsub();
    if (timer) clearInterval(timer);
  };
}
