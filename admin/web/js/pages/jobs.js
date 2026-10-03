// Every job (feat-ops-6; redesign-activity 3.71.0): every job the
// dashboard ran or runs, newest first, as a kp datatable inside Activity's
// folded "Every job" section. A row opens that job live in a dialog (its
// log scrolls inside the dialog, invariant 47).

import { act, actionLabel, loadJobs, onAct } from "../act.js";
import { agoEl, setAgo } from "../ago.js";
import { badgeCell, bindTableUrl, h, tableBlock, td } from "../dom.js";
import { formatDateTime } from "../format.js";
import { STATE_ORDER, jobRows } from "../jobs.js";
import { sortKeys } from "../sortkeys.js";
import { openJobDialog } from "./activitykit.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mountJobsTable(root) {
  const keys = sortKeys();
  const ago = agoEl("updated");
  const t = tableBlock({
    remember: "jobs",
    caption: "Every job, newest first (the last 200)",
    search: "Search jobs",
    state: "loading",
    nothing: "No jobs yet: an action from any stack's page starts one.",
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
  t.tbody.id = "jobs-table";
  root.replaceChildren(t.wrap, h("p", { class: "measured" }, ago));
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "jobs");

  t.tbody.addEventListener("click", (e) => {
    const target = /** @type {Element} */ (e.target);
    if (target.closest("a")) return;
    const id = Number(
      /** @type {HTMLElement | null} */ (target.closest("tr"))?.dataset.job,
    );
    if (Number.isInteger(id) && id > 0) openJobDialog(id);
  });

  let shown = "";
  if (!act.jobsRead)
    t.loading({ words: "Reading the jobs from the dashboard…" });
  const render = () => {
    if (!act.jobsRead) {
      if (act.failed.jobs) t.failed(act.failed.jobs);
      return;
    }
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
            title: "Open this job: its steps and its log",
          },
          td(String(r.job), "num"),
          td(keys.note("time", formatDateTime(r.queued), r.queued)),
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
    t.ready();
    setAgo(ago, now);
  };
  const offJobs = onAct("jobs", render);
  const retry = () => {
    t.loading({ words: "Reading the jobs from the dashboard…" });
    void loadJobs();
  };
  root.addEventListener("kp-datatable-retry", retry);
  const timer = setInterval(render, 5000);
  render();
  return () => {
    offJobs();
    root.removeEventListener("kp-datatable-retry", retry);
    clearInterval(timer);
    unbind();
    detach();
  };
}
