// Today (TUI parity, fix-68's `homelab today`): doctor, the fleet check
// with its manual checks and the open incidents as one list and one
// verdict; and the fleet check on its own (the TUI's c), with every finding
// and its remedy.

import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  h,
  slowRead,
  tableBlock,
  td,
} from "../dom.js";
import { humanDuration } from "../format.js";
import { findingRows, todayView } from "../parity.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const verdict = h("div", { id: "today-verdict", "aria-live": "polite" });
  const read = h(
    "button",
    { type: "button", class: "kp-button", id: "today-read" },
    "Read again",
  );
  const note = h("p", { class: "measured", role: "status" });
  const ago = agoEl("read");
  const items = tableBlock({
    remember: "today",
    caption: "What needs you",
    search: "Search",
    state: "loading",
    nothing:
      "Nothing needs you: doctor, the fleet check, the incidents and the manual checks are all clear.",
    columns: [
      {
        label: "Level",
        sort: "text",
        order: "broken,attention",
        filter: "choice",
      },
      { label: "From", sort: "text", filter: "choice" },
      { label: "What", sort: "text", cls: "wide" },
      { label: "What to do", sort: "text" },
    ],
  });
  const unread = h("ul", { class: "today-unread" });

  const checkBtn = h(
    "button",
    { type: "button", class: "kp-button", id: "fleet-check-run" },
    "Run the fleet check",
  );
  const checkAbout = h(
    "p",
    { class: "measured" },
    "The repository against the fleet (the TUI's c, homelab check): every stack file against what runs, backups, routes. The host needs about a minute and a half.",
  );
  const checkNote = h(
    "p",
    { class: "measured", role: "status" },
    "Not run yet on this page.",
  );
  const checkAgo = agoEl("read");
  const findings = tableBlock({
    remember: "fleet-check",
    caption: "Findings",
    search: "Search findings",
    nothing: "The fleet check found nothing.",
    columns: [
      {
        label: "Severity",
        sort: "text",
        order: "broken,drift,noted",
        filter: "choice",
      },
      { label: "About", sort: "text" },
      { label: "What", sort: "text", cls: "wide" },
      { label: "Remedy", sort: "text" },
    ],
  });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Today"), read),
    verdict,
    note,
    items.wrap,
    unread,
    h("p", null, ago),
    h("div", { class: "title-row" }, h("h2", null, "Fleet check"), checkBtn),
    checkAbout,
    checkNote,
    findings.wrap,
    h("p", null, checkAgo),
  );
  const detach = attachDataTables(root);
  const table = dataTable(items.wrap);
  const checkTable = dataTable(findings.wrap);
  const unbind = bindTableUrl(table, "today");
  const unbindC = bindTableUrl(checkTable, "findings");
  const abort = new AbortController();
  // Nothing to show before the first run: no empty table that looks like
  // a load that never finishes.
  findings.wrap.hidden = true;

  const load = async () => {
    read.disabled = true;
    // The table's status line counts the seconds; the verdict's place
    // says what is being asked until the verdict replaces it.
    items.loading({
      words:
        "Asking the host: doctor, the fleet check, the incidents and the manual checks…",
      expect: 93,
    });
    if (!verdict.querySelector(".today-verdict"))
      verdict.replaceChildren(
        h(
          "p",
          { class: "measured", role: "status" },
          "Today's list is being read on the host.",
        ),
      );
    let r;
    try {
      r = await slowRead("/data/today", "today", abort.signal);
    } finally {
      read.disabled = false;
    }
    if (!r.ok) {
      const took = humanDuration(items.failed(r.error));
      verdict.replaceChildren(
        h("p", { class: "measured", role: "status" }, `Failed after ${took}.`),
      );
      return;
    }
    const took = humanDuration(items.ready({ refresh: false }));
    const v = todayView(r.body);
    verdict.replaceChildren(
      h(
        "div",
        {
          class: `kp-alert kp-alert--${v.tone} today-verdict`,
          role: "status",
          "data-kp-semantic": "",
        },
        h("strong", null, v.verdict),
      ),
    );
    note.textContent = [v.note, `Read in ${took}.`].filter(Boolean).join(" ");
    note.hidden = false;
    items.tbody.replaceChildren(
      ...v.items.map((i) =>
        h(
          "tr",
          null,
          badgeCell(i.badge),
          td(i.source),
          td(i.what),
          td(i.remedy, "mono"),
        ),
      ),
    );
    unread.replaceChildren(
      ...v.unread.map((u) =>
        h("li", { class: "kp-alert kp-alert--warning" }, `Not read: ${u}`),
      ),
    );
    table?.refresh();
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };

  const runCheck = async () => {
    checkBtn.disabled = true;
    findings.wrap.hidden = false;
    findings.loading({
      words: "Running the fleet check on the host…",
      expect: 92,
    });
    checkNote.textContent = "The fleet check is running on the host.";
    let r;
    try {
      r = await slowRead("/data/fleet-check", "the fleet check", abort.signal);
    } finally {
      checkBtn.disabled = false;
    }
    if (!r.ok) {
      const took = humanDuration(findings.failed(r.error));
      checkNote.textContent = `The fleet check did not answer (after ${took}).`;
      return;
    }
    const took = humanDuration(findings.ready({ refresh: false }));
    const rows = findingRows(r.body.findings ?? []);
    findings.tbody.replaceChildren(
      ...rows.map((f) =>
        h(
          "tr",
          null,
          badgeCell(f.badge),
          td(f.subject),
          td(f.what),
          td(f.remedy, "mono"),
        ),
      ),
    );
    checkTable?.refresh();
    checkNote.textContent =
      `${r.body.passes ? "Passes" : "Does not pass"}: ${rows.length} finding(s) over ${r.body.stack_files} stack file(s), checked in ${took}. ${r.body.skipped ?? ""} ${r.body.not_here ?? ""}`.trim();
    setAgo(checkAgo, r.body.measured_at ?? Date.now() / 1000);
  };

  read.addEventListener("click", () => void load().catch(() => {}));
  checkBtn.addEventListener("click", () => void runCheck().catch(() => {}));
  const retry = (/** @type {Event} */ e) => {
    if (findings.wrap.contains(/** @type {Node} */ (e.target)))
      void runCheck().catch(() => {});
    else void load().catch(() => {});
  };
  root.addEventListener("kp-datatable-retry", retry);
  void load().catch(() => {});
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    unbindC();
    detach();
  };
}
