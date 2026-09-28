// Today (TUI parity, fix-68's `homelab today`): doctor, the fleet check
// with its manual checks and the open incidents as one list and one
// verdict; and the fleet check on its own (the TUI's c), with every finding
// and its remedy.

import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  errorBox,
  fetchJson,
  h,
  tableBlock,
  td,
} from "../dom.js";
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
  const err = h("div");
  const ago = agoEl("read");
  const items = tableBlock({
    remember: "today",
    caption: "What needs you",
    search: "Search",
    state: "loading",
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
  const checkNote = h(
    "p",
    { class: "measured", role: "status" },
    "The repository against the fleet (the TUI's c, homelab check): every stack file against what runs, backups, routes. About a minute.",
  );
  const checkErr = h("div");
  const checkAgo = agoEl("read");
  const findings = tableBlock({
    remember: "fleet-check",
    caption: "Findings",
    search: "Search findings",
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
    err,
    items.wrap,
    unread,
    h("p", null, ago),
    h("h2", null, "Fleet check"),
    h("div", { class: "title-row" }, checkNote, checkBtn),
    checkErr,
    findings.wrap,
    h("p", null, checkAgo),
  );
  const detach = attachDataTables(root);
  const table = dataTable(items.wrap);
  const checkTable = dataTable(findings.wrap);
  const unbind = bindTableUrl(table, "today");
  const unbindC = bindTableUrl(checkTable, "findings");
  const abort = new AbortController();

  const load = async () => {
    read.disabled = true;
    table?.state("loading");
    verdict.replaceChildren(
      h(
        "p",
        { class: "measured" },
        "Asking the host: doctor, the fleet check, incidents and manual checks (about a minute)…",
      ),
    );
    const r = await fetchJson("/data/today", "today", abort.signal);
    read.disabled = false;
    if (!r.ok) {
      err.replaceChildren(errorBox(r.error));
      verdict.replaceChildren();
      table?.state("failed");
      return;
    }
    err.replaceChildren();
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
    note.textContent = v.note;
    note.hidden = !v.note;
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
    table?.state("ready");
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };

  const runCheck = async () => {
    checkBtn.disabled = true;
    checkNote.textContent = "Checking… about a minute.";
    checkTable?.state("loading");
    const r = await fetchJson(
      "/data/fleet-check",
      "the fleet check",
      abort.signal,
    );
    checkBtn.disabled = false;
    checkBtn.textContent = "Run it again";
    if (!r.ok) {
      checkErr.replaceChildren(errorBox(r.error));
      checkTable?.state("failed");
      checkNote.textContent = "The fleet check did not answer.";
      return;
    }
    checkErr.replaceChildren();
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
    checkTable?.state("ready");
    checkNote.textContent =
      `${r.body.passes ? "Passes" : "Does not pass"}: ${rows.length} finding(s) over ${r.body.stack_files} stack file(s). ${r.body.skipped ?? ""} ${r.body.not_here ?? ""}`.trim();
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
