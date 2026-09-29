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
import { formatTime, humanDuration } from "../format.js";
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
    status,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "doctor");
  /** @type {AbortController | null} */
  let abort = null;

  const load = async () => {
    abort?.abort();
    const mine = new AbortController();
    abort = mine;
    const began = Date.now();
    refresh.setAttribute("disabled", "");
    // The table's status line counts; this line says what runs.
    t.loading({ words: "Asking the host to run the doctor…", expect: 30 });
    status.textContent = "The doctor is running on the host.";
    try {
      const r = await slowReport("/data/doctor", "the doctor", mine.signal);
      if (mine.signal.aborted) return;
      const took = humanDuration((Date.now() - began) / 1000);
      if (!r.ok) {
        t.failed(r.error);
        status.textContent = `Failed after ${took}.`;
        overall.replaceChildren();
        return;
      }
      const hl = health(r.report.overall);
      overall.className = `state ${hl.tone}`;
      overall.replaceChildren(h("span", null, `overall ${hl.label}`));
      status.textContent = `${doctorSummary(r.report)} · asked ${formatTime(began / 1000)}, took ${took}`;
      t.tbody.replaceChildren(
        ...doctorRows(r.report).map((x) =>
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
      t.ready();
      setAgo(ago, Date.now() / 1000);
    } finally {
      if (abort === mine) refresh.removeAttribute("disabled");
    }
  };

  refresh.addEventListener("click", () => void load().catch(() => {}));
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  void load().catch(() => {});
  return () => {
    abort?.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
