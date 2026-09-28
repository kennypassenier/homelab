// Jobs (feat-ops-6): every job the dashboard ran or runs, newest first, as
// a kp datatable; the one in `?job=` gets its live panel above the table.

import { act, actionLabel, onAct } from "../act.js";
import { agoEl, setAgo } from "../ago.js";
import { badgeCell, bindTableUrl, h, tableBlock, td } from "../dom.js";
import { formatTime } from "../format.js";
import { STATE_ORDER, jobRows } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { sortKeys } from "../sortkeys.js";
import { setParams } from "../urlstate.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} ctx
 * @returns {() => void}
 */
export function mount(root, ctx) {
  const keys = sortKeys();
  const selected = h("div", { id: "job-selected" });
  const ago = agoEl("updated");
  const t = tableBlock({
    remember: "jobs",
    caption: "Jobs, newest first (the last 200)",
    search: "Search jobs",
    columns: [
      { label: "Job", sort: "number" },
      { label: "Queued", sort: "time" },
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "Action", sort: "text", filter: "choice" },
      { label: "From", sort: "text", filter: "choice" },
      { label: "State", sort: "text", order: STATE_ORDER, filter: "choice" },
      { label: "Took", sort: "duration" },
      { label: "Step", sort: "text" },
      { label: "Message", sort: "text", cls: "wide" },
    ],
  });
  t.tbody.id = "jobs";
  root.replaceChildren(
    h("h1", null, "Jobs"),
    h(
      "p",
      { class: "measured" },
      "What the dashboard sent to the host, live. Pick a row to follow that job.",
    ),
    selected,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "jobs");

  /** @type {{job: number, stop: () => void} | null} */
  let panel = null;
  const showJob = () => {
    const id = Number(new URLSearchParams(location.search).get("job"));
    if (!Number.isInteger(id) || id <= 0) {
      panel?.stop();
      panel = null;
      selected.replaceChildren();
      return;
    }
    if (panel?.job === id) return;
    panel?.stop();
    const p = mountJobPanel(id);
    panel = { job: id, stop: p.stop };
    const close = h(
      "button",
      { type: "button", class: "kp-button kp-button--ghost" },
      "Stop following",
    );
    close.addEventListener("click", () => {
      history.replaceState(
        history.state,
        "",
        location.pathname + setParams(location.search, { job: null }),
      );
      showJob();
    });
    selected.replaceChildren(p.element, close);
  };

  t.tbody.addEventListener("click", (e) => {
    const target = /** @type {Element} */ (e.target);
    if (target.closest("a")) return;
    const tr = target.closest("tr");
    const id = tr?.dataset.job;
    if (!id) return;
    history.replaceState(
      history.state,
      "",
      location.pathname + setParams(location.search, { job: id }),
    );
    showJob();
    window.scrollTo(0, 0);
  });

  let shown = "";
  const render = () => {
    const now = Date.now() / 1000;
    const rows = jobRows(act.jobs, actionLabel, now);
    // Redraw only when something a row shows changed (a running job's
    // duration ticks with the second anyway).
    const sig = JSON.stringify(
      rows.map((r) => [
        r.job,
        r.badge.label,
        r.step,
        r.message,
        r.took == null,
      ]),
    );
    const running = rows.some((r) => r.badge.label === "running");
    if (sig === shown && !running) return;
    shown = sig;
    t.tbody.replaceChildren(
      ...rows.map((r) =>
        h(
          "tr",
          {
            class: "link-row",
            "data-job": String(r.job),
            "data-kp-row-key": String(r.job),
          },
          td(String(r.job), "num"),
          td(keys.note("time", formatTime(r.queued), r.queued)),
          td(r.stack),
          td(r.action),
          td(r.origin),
          badgeCell(r.badge),
          td(
            r.took == null ? "—" : keys.note("duration", r.tookText, r.took),
            "num",
          ),
          td(r.step),
          td(r.message),
        ),
      ),
    );
    table?.refresh();
    setAgo(ago, now);
  };
  const offJobs = onAct("jobs", render);
  const timer = setInterval(render, 5000);
  render();
  showJob();
  void ctx;
  return () => {
    offJobs();
    clearInterval(timer);
    panel?.stop();
    unbind();
    detach();
  };
}
