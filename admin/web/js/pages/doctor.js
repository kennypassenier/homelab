// Doctor: the host's self-diagnosis. It reads for about half a minute on
// pve, so the page says so while it waits and offers a Refresh.

import { doctorRows, doctorSummary, health } from "../doctor.js";
import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  slowReport,
  tableBlock,
  td,
} from "../dom.js";
import { formatDateTime, humanDuration } from "../format.js";
import { fetchAnnounced, keepRead, keptRead, runUrl } from "../slowread.js";
import { listen } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const t = tableBlock({
    remember: "doctor",
    caption: "Doctor checks",
    search: "Search checks",
    state: "loading",
    nothing: "The doctor answered with no checks.",
    columns: [
      { label: "Check", sort: "text" },
      {
        label: "Health",
        sort: "text",
        order: "fail,warn,ok",
        filter: "choice",
      },
      { label: "Detail", sort: "text", cls: "wide" },
      { label: "Remedy", sort: "text" },
    ],
  });
  const refresh = h(
    "button",
    { class: "kp-button kp-button--primary", type: "button" },
    "Refresh",
  );
  const overall = h("span", { class: "state" });
  const status = h("p", { class: "measured", role: "status" });
  const ago = agoEl("read");
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Doctor"), overall, refresh),
    h(
      "p",
      { class: "section-head__desc measured" },
      "A deeper diagnostic run across the fleet, checked on request — slower than the Today block's quick read.",
    ),
    status,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "doctor");
  /** @type {AbortController | null} */
  let abort = null;

  const page = { shown: /** @type {number | null} */ (null), reading: false };
  const words = { words: "Asking the host to run the doctor…", expect: 30 };

  /**
   * Show one answer of /data/doctor.
   * @param {any} body the route's whole answer (`report` inside)
   * @param {{began?: number, last?: boolean}} o last: an earlier answer,
   *   shown while a new run reads again
   */
  const paint = (body, o = {}) => {
    const report = body.report;
    const hl = health(report.overall);
    overall.className = `state ${hl.tone}`;
    overall.replaceChildren(h("span", null, `overall ${hl.label}`));
    status.textContent =
      o.began != null && !o.last
        ? `${doctorSummary(report)} · asked ${formatDateTime(o.began / 1000)}, took ${humanDuration((Date.now() - o.began) / 1000)}`
        : `${doctorSummary(report)} · the last reading${o.last ? ", while the host runs the doctor again" : ""}`;
    t.tbody.replaceChildren(
      ...doctorRows(report).map((x) =>
        h(
          "tr",
          null,
          td(x.name),
          badgeCell(x.health),
          td(x.detail),
          td(x.remedy || "—"),
        ),
      ),
    );
    setAgo(ago, body.read_at ?? Date.now() / 1000);
    page.shown = body.read_run ?? null;
    if (!o.last) keepRead("/data/doctor", body);
  };

  /**
   * An earlier answer, on screen now; the table then shows the new run.
   * @param {any} body
   */
  const showLast = (body) => {
    paint(body, { last: true });
    t.ready();
    // The last answer stays readable: the big layer is for a table with
    // nothing to show yet.
    t.loading({ ...words, overlay: false });
  };

  /** @param {string} [url] a run's own url, for a `slow_read` event */
  const load = async (url) => {
    abort?.abort();
    const mine = new AbortController();
    abort = mine;
    const began = Date.now();
    refresh.setAttribute("disabled", "");
    page.reading = true;
    // A page this tab showed before paints at once; the dashboard's own
    // last answer follows within a request.
    const kept = url ? undefined : keptRead("/data/doctor");
    if (kept) showLast(kept);
    // The table's status line counts; this line says what runs.
    else {
      t.loading(words);
      status.textContent = "The doctor is running on the host.";
    }
    try {
      const r = await slowReport(
        url ?? "/data/doctor",
        "the doctor",
        mine.signal,
        showLast,
      );
      if (mine.signal.aborted) return;
      if (!r.ok) {
        const took = humanDuration((Date.now() - began) / 1000);
        t.failed(r.error);
        status.textContent = `Failed after ${took}.`;
        overall.replaceChildren();
        return;
      }
      paint(r.body, { began });
      t.ready();
    } finally {
      if (abort === mine) {
        refresh.removeAttribute("disabled");
        page.reading = false;
      }
    }
  };

  // A run finished (this tab's, another tab's, the Host page's): fetch it
  // by id, which starts nothing on the host.
  const unlisten = listen("slow_read", (ev) => {
    if (fetchAnnounced(ev, "doctor", page))
      void load(runUrl("/data/doctor", ev.run)).catch(() => {});
  });

  refresh.addEventListener("click", () => void load().catch(() => {}));
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  void load().catch(() => {});
  return () => {
    abort?.abort();
    unlisten();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
