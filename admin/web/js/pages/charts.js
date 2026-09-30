// Charts (replace-grafana, Kenny 2026-09-30: "liefst Grafana vervangen door
// ons dashboard zodat ik maar 1 plek heb om naar te kijken"). The host's or
// one stack's panels over a window, read by the dashboard's server from
// Prometheus; the stack and the window live in the address.

import { H, W, formatValue, layout } from "../charts.js";
import { h, fetchJson } from "../dom.js";
import { formatTime } from "../format.js";
import { current, subscribe } from "../store.js";
import { choice, setParams } from "../urlstate.js";

const RANGES = /** @type {const} */ (["1h", "6h", "24h", "7d", "30d"]);
const NS = "http://www.w3.org/2000/svg";

/**
 * @param {string} tag
 * @param {Record<string, string>} attrs
 * @param {(Node | string)[]} kids
 */
function svg(tag, attrs, ...kids) {
  const el = document.createElementNS(NS, tag);
  for (const [k, v] of Object.entries(attrs)) el.setAttribute(k, v);
  for (const k of kids) el.append(k);
  return el;
}

/**
 * One panel: title, the chart, and each series' latest value.
 * @param {any} p `{panel, series, error?}` from /data/charts
 * @param {number} from
 * @param {number} to
 */
export function panelEl(p, from, to) {
  const unit = p.panel.unit;
  const box = h(
    "figure",
    { class: "chart" },
    h("figcaption", null, p.panel.title),
  );
  if (p.error) {
    box.append(h("p", { class: "chart__error" }, p.error));
    return box;
  }
  if (
    !p.series.length ||
    p.series.every((/** @type {any} */ s) => !s.points.length)
  ) {
    box.append(h("p", { class: "chart__empty" }, "No data in this window."));
    return box;
  }
  const L = layout(p.series, from, to, unit);
  const plot = svg("svg", {
    viewBox: `0 0 ${W} ${H}`,
    class: "chart__svg",
    role: "img",
    "aria-label": p.panel.title,
  });
  for (const t of L.yTicks) {
    plot.append(
      svg("line", {
        x1: String(L.x0),
        x2: String(L.x1),
        y1: String(t.y),
        y2: String(t.y),
        class: "chart__grid",
      }),
      svg(
        "text",
        {
          x: String(L.x0 - 6),
          y: String(t.y + 4),
          class: "chart__tick",
          "text-anchor": "end",
        },
        t.label,
      ),
    );
  }
  plot.append(
    svg(
      "text",
      { x: String(L.x0), y: String(H - 4), class: "chart__tick" },
      formatTime(from),
    ),
    svg(
      "text",
      {
        x: String(L.x1),
        y: String(H - 4),
        class: "chart__tick",
        "text-anchor": "end",
      },
      formatTime(to),
    ),
  );
  L.paths.forEach((s, i) =>
    plot.append(
      svg("path", { d: s.d, class: `chart__line chart__line--${i % 6}` }),
    ),
  );
  box.append(plot);
  const legend = h("ul", { class: "chart__legend" });
  L.paths.forEach((s, i) =>
    legend.append(
      h(
        "li",
        { class: `chart__key chart__key--${i % 6}` },
        `${s.label || "now"}: ${s.last == null ? "—" : formatValue(s.last, unit)}`,
      ),
    ),
  );
  box.append(legend);
  return box;
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  const params = new URLSearchParams(location.search);
  const stack = params.get("stack") ?? "";
  const range = choice(params, "range", RANGES, "24h");
  const pick = h("select", {
    class: "kp-field__input",
    "aria-label": "Which charts",
  });
  const fill = () => {
    const names = (current().fleet?.stacks ?? []).map((s) => s.name).sort();
    pick.replaceChildren(
      h("option", { value: "" }, "The host"),
      ...names.map((n) => h("option", { value: n }, `Stack ${n}`)),
    );
    pick.value = stack;
  };
  fill();
  const unsub = subscribe(fill);
  pick.addEventListener("change", () =>
    ctx.navigate(
      `/app/charts${setParams(location.search, { stack: pick.value })}`,
    ),
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
        ctx.navigate(`/app/charts${setParams(location.search, { range: r })}`),
      );
      return b;
    }),
  );
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the charts…",
  );
  const grid = h("div", { class: "chart-grid" });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Charts"), pick, ranges),
    status,
    grid,
  );
  const abort = new AbortController();
  (async () => {
    const q = setParams("", { stack: stack || null, range });
    const r = await fetchJson(`/data/charts${q}`, "the charts", abort.signal);
    if (!r.ok) {
      status.textContent = `${r.error.what}: ${r.error.why} — ${r.error.fix}`;
      return;
    }
    status.textContent = `${stack ? `Stack ${stack}` : "The host"}, ${formatTime(r.body.from)} to ${formatTime(r.body.to)}`;
    grid.replaceChildren(
      ...r.body.panels.map((/** @type {any} */ p) =>
        panelEl(p, r.body.from, r.body.to),
      ),
    );
  })().catch(() => {});
  return () => {
    abort.abort();
    unsub();
  };
}
