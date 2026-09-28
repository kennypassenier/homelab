// Manual checks (feat-ops-3): the questions deploys left for a person,
// open ones first. Answering comes with the actions milestone.

import { checkRows } from "../checks.js";
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
import { formatTime } from "../format.js";
import { sortKeys } from "../sortkeys.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const keys = sortKeys();
  /** @param {number | null} unix */
  const time = (unix) =>
    unix == null ? "never" : keys.note("time", formatTime(unix), unix);
  const t = tableBlock({
    remember: "checks",
    caption: "Manual checks",
    search: "Search checks",
    state: "loading",
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "App", sort: "text" },
      {
        label: "Answer",
        sort: "text",
        order: "not ok,open,accepted,ok",
        filter: "choice",
      },
      { label: "Question", sort: "text", cls: "wide" },
      { label: "Answered", sort: "time" },
      { label: "Registered", sort: "time" },
      { label: "Note", sort: "text" },
      { label: "More", sort: "text" },
    ],
  });
  const err = h("div");
  const summary = h("p", { class: "measured" });
  const ago = agoEl("read");
  root.replaceChildren(
    h("h1", null, "Checks"),
    err,
    summary,
    t.wrap,
    h("p", null, ago),
  );
  const detach = attachDataTables(root, { compare: keys.compare(compare) });
  const table = dataTable(t.wrap);
  const unbind = bindTableUrl(table, "checks");
  const abort = new AbortController();

  const load = async () => {
    table?.state("loading");
    const r = await fetchReport(
      "/data/manual-checks",
      "the manual checks",
      abort.signal,
    );
    if (!r.ok) {
      err.replaceChildren(errorBox(r.error));
      table?.state("failed");
      return;
    }
    err.replaceChildren();
    const now = r.report?.now ?? Math.floor(Date.now() / 1000);
    const rows = checkRows(r.report?.checks ?? [], now);
    const open = rows.filter((x) => x.answer.label !== "ok").length;
    summary.textContent = `${rows.length} checks, ${open} not answered ok`;
    t.tbody.replaceChildren(
      ...rows.map((x) =>
        h(
          "tr",
          null,
          td(x.stack),
          td(x.app),
          badgeCell(x.answer),
          td(x.text),
          td(time(x.answered)),
          td(time(x.registered)),
          td(x.note),
          td(x.extras, "mono"),
        ),
      ),
    );
    table?.refresh();
    table?.state("ready");
    setAgo(ago, Date.now() / 1000);
  };
  const retry = () => void load();
  root.addEventListener("kp-datatable-retry", retry);
  void load().catch(() => {});
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    detach();
  };
}
