// Activity (feat-ops-1, feat-ops-7): the incident bundles and what the
// host did in the last fourteen days.

import { historyRows, incidentRows } from "../activity.js";
import { showButton } from "../incident.js";
import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  fetchReport,
  h,
  tableBlock,
  td,
} from "../dom.js";
import { formatTime, humanDuration } from "../format.js";
import { sortKeys } from "../sortkeys.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";

const DAYS = 14;

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const keys = sortKeys();
  /** @param {number | null} unix */
  const time = (unix) =>
    unix == null ? "—" : keys.note("time", formatTime(unix), unix);
  const inc = tableBlock({
    remember: "incidents",
    caption: "Incidents",
    search: "Search incidents",
    state: "loading",
    nothing: "No incidents: no operation has failed.",
    columns: [
      { label: "When", sort: "time" },
      { label: "Operation", sort: "text" },
      { label: "Bundle", sort: "text" },
      { label: "Read", sort: "text" },
    ],
  });
  const hist = tableBlock({
    remember: "history",
    caption: `History, last ${DAYS} days`,
    search: "Search history",
    state: "loading",
    nothing: `Nothing ran in the last ${DAYS} days.`,
    columns: [
      { label: "Started", sort: "time" },
      { label: "What", sort: "text" },
      { label: "Took", sort: "duration" },
      {
        label: "Outcome",
        sort: "text",
        order: "failed,running,deferred,ok,done",
        filter: "choice",
      },
      { label: "By", sort: "text", filter: "choice" },
      { label: "Detail", sort: "text" },
    ],
  });
  const incAgo = agoEl("read");
  const histAgo = agoEl("read");
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Activity"),
      h(
        "a",
        { href: "/app/timeline", class: "kp-button" },
        "See it on the timeline",
      ),
    ),
    h("h2", null, "Incidents"),
    inc.wrap,
    h("p", null, incAgo),
    h("h2", null, "History"),
    hist.wrap,
    h("p", null, histAgo),
  );
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const incTable = dataTable(inc.wrap);
  const histTable = dataTable(hist.wrap);
  const unbindI = bindTableUrl(incTable, "incidents");
  const unbindH = bindTableUrl(histTable, "history");
  const abort = new AbortController();

  const loadIncidents = async () => {
    inc.loading({ words: "Reading the incidents from the host…" });
    const r = await fetchReport(
      "/data/incidents",
      "the incidents",
      abort.signal,
    );
    if (!r.ok) {
      inc.failed(r.error);
      return;
    }
    const names = /** @type {string[]} */ (r.report?.incidents ?? []);
    inc.tbody.replaceChildren(
      ...incidentRows(names).map((x) =>
        h(
          "tr",
          null,
          td(time(x.at)),
          td(x.op),
          td(x.name, "mono"),
          showButton(x.name),
        ),
      ),
    );
    inc.ready();
    setAgo(incAgo, Date.now() / 1000);
  };

  const loadHistory = async () => {
    hist.loading({ words: "Reading the history from the host…" });
    const since = Math.floor(Date.now() / 1000) - DAYS * 86400;
    const r = await fetchReport(
      `/data/history?since=${since}`,
      "the history",
      abort.signal,
    );
    if (!r.ok) {
      hist.failed(r.error);
      return;
    }
    const entries = r.report?.entries ?? [];
    hist.tbody.replaceChildren(
      ...historyRows(entries).map((x) =>
        h(
          "tr",
          null,
          td(time(x.start)),
          td(x.what),
          td(
            x.took == null
              ? "—"
              : keys.note("duration", humanDuration(x.took), x.took),
            "num",
          ),
          badgeCell(x.outcome),
          td(x.by),
          td(x.detail),
        ),
      ),
    );
    hist.ready();
    setAgo(histAgo, Date.now() / 1000);
  };

  const retry = (/** @type {Event} */ e) => {
    if (inc.wrap.contains(/** @type {Node} */ (e.target))) void loadIncidents();
    else void loadHistory();
  };
  root.addEventListener("kp-datatable-retry", retry);
  void loadIncidents().catch(() => {});
  void loadHistory().catch(() => {});
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    unbindI();
    unbindH();
    detach();
  };
}
