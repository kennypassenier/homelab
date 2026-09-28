// Doctor: the host's self-diagnosis. It reads for about half a minute on
// pve, so the page says so while it waits and offers a Refresh.

import { doctorRows, doctorSummary, health } from "../doctor.js";
import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  errorBox,
  fetchReport,
  h,
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
  const err = h("div");
  const ago = agoEl("read");
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Doctor"), overall, refresh),
    status,
    err,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root);
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "doctor");
  /** @type {AbortController | null} */
  let abort = null;
  /** @type {ReturnType<typeof setInterval> | undefined} */
  let timer;

  const load = async () => {
    abort?.abort();
    const mine = new AbortController();
    abort = mine;
    const began = Date.now();
    refresh.setAttribute("disabled", "");
    table?.state("loading");
    err.replaceChildren();
    const waiting = () => {
      const s = Math.round((Date.now() - began) / 1000);
      status.textContent = `Asking the host… ${humanDuration(s)} so far; the doctor takes about 30 s.`;
    };
    waiting();
    clearInterval(timer);
    timer = setInterval(waiting, 1000);
    try {
      const r = await fetchReport("/data/doctor", "the doctor", mine.signal);
      if (mine.signal.aborted) return;
      const took = humanDuration((Date.now() - began) / 1000);
      if (!r.ok) {
        err.replaceChildren(errorBox(r.error));
        status.textContent = `Failed after ${took}.`;
        overall.replaceChildren();
        table?.state("failed");
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
      table?.refresh();
      table?.state("ready");
      setAgo(ago, Date.now() / 1000);
    } finally {
      if (abort === mine) {
        clearInterval(timer);
        refresh.removeAttribute("disabled");
      }
    }
  };

  refresh.addEventListener("click", () => void load().catch(() => {}));
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  void load().catch(() => {});
  return () => {
    abort?.abort();
    clearInterval(timer);
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
