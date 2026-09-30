// Traffic (replace-goaccess, Kenny 2026-09-30): who visits the services from
// outside, from the proxy's access log in Loki (a stack's `log_files:` ships
// it). Requests per hostname and per status over the window, and the
// busiest hostnames and client addresses in it; the page reads again every
// half minute, the way GoAccess's counter moved.

import { h, fetchJson } from "../dom.js";
import { formatTime } from "../format.js";
import { choice, setParams } from "../urlstate.js";
import { panelEl } from "./charts.js";

const RANGES = /** @type {const} */ (["1h", "6h", "24h", "7d"]);

/**
 * A two-column table of the busiest names in the window.
 * @param {string} caption
 * @param {string} what
 * @param {{rows: [string, number][], error?: string}} t
 */
function topTable(caption, what, t) {
  if (t.error)
    return h("p", { class: "chart__error" }, `${caption}: ${t.error}`);
  return h(
    "table",
    { class: "kp-table" },
    h("caption", null, caption),
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
  const range = choice(params, "range", RANGES, "24h");
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
        ctx.navigate(`/app/traffic${setParams(location.search, { range: r })}`),
      );
      return b;
    }),
  );
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the access log…",
  );
  const grid = h("div", { class: "chart-grid" });
  const tables = h("div", { class: "chart-grid" });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Traffic"), ranges),
    status,
    grid,
    tables,
  );
  const abort = new AbortController();
  const read = async () => {
    const r = await fetchJson(
      `/data/traffic${setParams("", { range })}`,
      "the traffic",
      abort.signal,
    );
    if (!r.ok) {
      status.textContent = `${r.error.what}: ${r.error.why} — ${r.error.fix}`;
      return;
    }
    const b = r.body;
    status.textContent = `${formatTime(b.from)} to ${formatTime(b.to)} · reads again every 30 s`;
    grid.replaceChildren(
      ...b.panels.map((/** @type {any} */ p) => panelEl(p, b.from, b.to)),
    );
    tables.replaceChildren(
      topTable("Busiest hostnames", "Hostname", b.hosts),
      topTable("Busiest client addresses", "Client", b.clients),
    );
  };
  read().catch(() => {});
  const timer = setInterval(() => read().catch(() => {}), 30_000);
  return () => {
    clearInterval(timer);
    abort.abort();
  };
}
