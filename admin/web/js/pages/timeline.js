// The timeline (feat-ops-7): everything the host did and every incident, on
// one time axis, drawn as SVG by this module (tech-charts: no chart
// library, colours from the kp-themes tokens). The window is in the
// address (?days=7).

import { declareField } from "../drivable.js";
import { agoEl, setAgo } from "../ago.js";
import { errorBox, fetchReport, h } from "../dom.js";
import { timelineModel } from "../timeline.js";
import { choice, setParams } from "../urlstate.js";

// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const TIMELINE_DAYS = declareField({
  id: "timeline-days",
  page: "timeline",
  what: "how many days the timeline shows",
});

const SVG = "http://www.w3.org/2000/svg";
const DAYS = /** @type {const} */ (["1", "3", "7", "14", "30"]);

/**
 * fix-251 (design review, 2026-10-03): marks are filled with the
 * foreground-strength tokens. kp-themes' `--success`/`--warning` are the
 * plates a badge's text sits on (dark: hsl(155 40% 16%) on a near-black
 * page) and all but vanish as a fill; `--destructive` and `--chart-1` are
 * already full-strength.
 * @type {Record<string, string>}
 */
const FILL = {
  ok: "var(--success-foreground)",
  warn: "var(--warning-foreground)",
  bad: "var(--destructive)",
  info: "var(--chart-1, var(--primary))",
};

/**
 * @param {string} tag
 * @param {Record<string, string | number>} attrs
 * @param {...(Node | string)} children
 */
function s(tag, attrs, ...children) {
  const e = document.createElementNS(SVG, tag);
  for (const [k, v] of Object.entries(attrs)) e.setAttribute(k, String(v));
  e.append(...children);
  return e;
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  let days = choice(new URLSearchParams(location.search), "days", DAYS, "7");
  const daySel = h(
    "select",
    { class: "kp-field__input", id: TIMELINE_DAYS },
    ...DAYS.map((d) =>
      h("option", { value: d }, d === "1" ? "1 day" : `${d} days`),
    ),
  );
  daySel.value = days;
  const chart = h(
    "div",
    { class: "timeline", id: "timeline" },
    h("p", { class: "measured" }, "Reading the history and the incidents…"),
  );
  const detail = h(
    "p",
    { class: "timeline-detail", id: "timeline-detail", role: "status" },
    "Point at or tab to a mark to read it.",
  );
  const summary = h("p", { class: "measured" }, "Reading the timeline…");
  const ago = agoEl("read");
  const err = h("div");
  const legend = h(
    "ul",
    { class: "timeline-legend", "aria-label": "Legend" },
    ...[
      ["ok", "succeeded"],
      ["bad", "failed, or an incident"],
      ["warn", "running or deferred"],
      ["info", "nightly round"],
    ].map(([tone, label]) =>
      h("li", null, h("span", { class: `swatch ${tone}` }), label),
    ),
  );
  root.replaceChildren(
    h("h1", null, "Timeline"),
    h(
      "p",
      { class: "section-head__desc measured" },
      "Backups and deploys charted over time, so a gap or a cluster stands out.",
    ),
    h(
      "div",
      { class: "title-row" },
      h(
        "div",
        { class: "kp-field" },
        h(
          "label",
          { class: "kp-field__label", for: "timeline-days" },
          "Window",
        ),
        daySel,
      ),
      legend,
    ),
    err,
    chart,
    detail,
    summary,
    h("p", null, ago),
  );

  /** @type {{entries: any[], incidents: string[], from: number, to: number} | null} */
  let data = null;
  const abort = new AbortController();

  const draw = () => {
    if (!data) return;
    const m = timelineModel({ ...data, width: chart.clientWidth || 800 });
    const svg = s("svg", {
      viewBox: `0 0 ${m.width} ${m.height}`,
      width: m.width,
      height: m.height,
      role: "img",
      "aria-label": `Timeline of the last ${days} days`,
    });
    const axisY = m.height - 20;
    for (const l of m.lanes) {
      svg.append(
        s("rect", {
          x: 0,
          y: l.y - 2,
          width: m.width,
          height: l.h + 4,
          class: "lane",
        }),
        s("text", { x: 4, y: l.y + 13, class: "lane-label" }, l.label),
      );
    }
    for (const t of m.ticks) {
      svg.append(
        s("line", {
          x1: t.x,
          x2: t.x,
          y1: 0,
          y2: axisY,
          class: t.major ? "tick major" : "tick",
        }),
        s("text", { x: t.x + 3, y: axisY + 14, class: "tick-label" }, t.label),
      );
    }
    const list = s("g", { role: "list", "aria-label": "Events" });
    for (const k of m.marks) {
      const g = s("g", {
        role: "listitem",
        tabindex: 0,
        class: `mark ${k.kind}`,
        "aria-label": k.title,
        "data-kind": k.kind,
      });
      if (k.kind === "incident") {
        const cx = k.x;
        const cy = k.y + k.h / 2;
        g.append(
          s("path", {
            d: `M${cx} ${cy - 7} L${cx + 7} ${cy} L${cx} ${cy + 7} L${cx - 7} ${cy} Z`,
            style: `fill: ${FILL[k.tone]}`,
          }),
        );
      } else {
        g.append(
          s("rect", {
            x: k.x,
            y: k.y,
            width: k.w,
            height: k.h,
            rx: 3,
            style: `fill: ${FILL[k.tone]}`,
          }),
        );
      }
      g.append(s("title", {}, k.title));
      const show = () => (detail.textContent = k.title);
      g.addEventListener("mouseenter", show);
      g.addEventListener("focus", show);
      g.addEventListener("click", show);
      list.append(g);
    }
    svg.append(list);
    chart.replaceChildren(svg);
    summary.textContent = `${m.counts.ops} operations, ${m.counts.phases} nightly phases, ${m.counts.incidents} incidents in the last ${days === "1" ? "day" : `${days} days`}.`;
  };

  const load = async () => {
    const to = Math.floor(Date.now() / 1000);
    const from = to - Number(days) * 86400;
    const [hr, ir] = await Promise.all([
      fetchReport(
        `/data/history?since=${from}&limit=5000`,
        "the history",
        abort.signal,
      ),
      fetchReport("/data/incidents", "the incidents", abort.signal),
    ]);
    err.replaceChildren(
      ...(hr.ok ? [] : [errorBox(hr.error)]),
      ...(ir.ok ? [] : [errorBox(ir.error)]),
    );
    data = {
      entries: hr.ok ? (hr.report?.entries ?? []) : [],
      incidents: ir.ok ? (ir.report?.incidents ?? []) : [],
      from,
      to,
    };
    draw();
    setAgo(ago, to);
  };

  daySel.addEventListener("change", () => {
    days = choice(
      new URLSearchParams(`days=${daySel.value}`),
      "days",
      DAYS,
      "7",
    );
    const search = setParams(location.search, {
      days: days === "7" ? null : days,
    });
    history.replaceState(history.state, "", location.pathname + search);
    void load().catch(() => {});
  });
  const resize = new ResizeObserver(() => draw());
  resize.observe(chart);
  void load().catch(() => {});
  void ctx;
  return () => {
    abort.abort();
    resize.disconnect();
  };
}
