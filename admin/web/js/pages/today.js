// Today (TUI parity, fix-68's `homelab today`): doctor, the fleet check
// with its manual checks and the open incidents as one list and one
// verdict; and the fleet check on its own (the TUI's c), with every finding
// and its remedy. A remedy the dashboard runs has a Fix button that opens
// the action's dialog, prefilled (Kenny, 2026-09-30).

import { agoEl, setAgo } from "../ago.js";
import {
  badgeCell,
  bindTableUrl,
  fetchJson,
  h,
  slowRead,
  tableBlock,
  td,
} from "../dom.js";
import { humanDuration } from "../format.js";
import { openAction } from "../actiondialog.js";
import { fixLabel } from "../notices.js";
import { findingRows, todayView } from "../parity.js";
import { fetchAnnounced, keepRead, keptRead, runUrl } from "../slowread.js";
import { listen } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

/**
 * A remedy, and its Fix button when the dashboard runs it: the action's own
 * dialog opens prefilled, and its review and Confirm decide.
 * @param {string} remedy
 * @param {import("../notices.js").Fix | null} fix
 */
function remedyCell(remedy, fix) {
  if (!fix) return td(remedy, "mono");
  const b = h(
    "button",
    { type: "button", class: "kp-button kp-button--sm fix-button" },
    fixLabel(fix),
  );
  b.addEventListener(
    "click",
    () => void openAction(fix.stack, fix.action, { preset: fix.args ?? {} }),
  );
  return h("td", null, h("span", { class: "mono" }, remedy), " ", b);
}

/**
 * One file's part of a drift diff: its path and sign, then its change in
 * monospace — or "(secret — content not shown)" for a path that never
 * carries content (fix-219).
 * @param {{path: string, sign: string, diff: string | null}} f
 */
function fileDiffBlock(f) {
  return h(
    "div",
    { class: "finding-diff__file" },
    h("div", { class: "mono" }, `${f.sign} ${f.path}`),
    f.diff == null
      ? h("p", { class: "measured" }, "(secret — content not shown)")
      : h("pre", { class: "mono" }, f.diff),
  );
}

/**
 * The "About" cell for a repo-drift finding (fix-219,
 * drift-finding-names-only-filenames): the stack name inside a `<details>`
 * that reads its per-file diff from the host, lazily, the first time it is
 * opened — rule 8: the summary says what opening it does before it is
 * pressed. A plain finding (not a repo-drift one) still gets a plain cell.
 * @param {string} stack
 */
function driftDiffCell(stack) {
  const body = h(
    "div",
    { class: "finding-diff__body", hidden: "" },
    "Reading the diff from the host…",
  );
  const details = h(
    "details",
    { class: "finding-diff" },
    h(
      "summary",
      null,
      stack,
      " ",
      h("span", { class: "measured" }, "(show what changed, file by file)"),
    ),
    body,
  );
  let loaded = false;
  details.addEventListener("toggle", () => {
    if (!details.open || loaded) return;
    loaded = true;
    body.hidden = false;
    void fetchJson(
      `/data/fleet-check/${encodeURIComponent(stack)}/diff`,
      `the diff of ${stack}`,
    ).then((r) => {
      if (!r.ok) {
        body.replaceChildren(
          h(
            "p",
            { class: "measured", role: "status" },
            `Could not read the diff: ${r.error.why}.`,
          ),
        );
        return;
      }
      const files = r.body.files ?? [];
      body.replaceChildren(
        files.length
          ? h("div", null, ...files.map(fileDiffBlock))
          : h("p", { class: "measured" }, "No files to show."),
      );
    });
  });
  return h("td", null, details);
}

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
    h("div", { class: "title-row" }, h("h1", null, "Today"), ago, read),
    verdict,
    note,
    items.wrap,
    unread,
    h(
      "div",
      { class: "title-row" },
      h("h2", null, "Fleet check"),
      checkAgo,
      checkBtn,
    ),
    checkAbout,
    checkNote,
    findings.wrap,
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

  /** The run whose answer each part shows, and whether it is reading. */
  const todayPage = {
    shown: /** @type {number | null} */ (null),
    reading: false,
  };
  const checkPage = {
    shown: /** @type {number | null} */ (null),
    reading: false,
  };

  /**
   * Show one answer of /data/today.
   * @param {any} body
   * @param {{took?: string, last?: boolean}} o last: an earlier answer,
   *   shown while a new run reads again
   */
  const paintToday = (body, o = {}) => {
    const v = todayView(body);
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
    // How long the read took is shown once, beside the title (`ago`'s own
    // line already says how long ago that was) — not repeated here too
    // (Kenny, 2026-10-02: "Read in 0 s." above the table and "read 0 s
    // ago" below it said the same thing twice).
    note.textContent = [
      v.note,
      o.last ? "The last reading, while the host reads again." : "",
    ]
      .filter(Boolean)
      .join(" ");
    note.hidden = false;
    items.tbody.replaceChildren(
      ...v.items.map((i) =>
        h(
          "tr",
          null,
          badgeCell(i.badge),
          td(i.source),
          td(i.what),
          remedyCell(i.remedy, i.fix),
        ),
      ),
    );
    unread.replaceChildren(
      ...v.unread.map((u) =>
        h("li", { class: "kp-alert kp-alert--warning" }, `Not read: ${u}`),
      ),
    );
    table?.refresh();
    setAgo(ago, body.read_at ?? body.measured_at ?? Date.now() / 1000);
    todayPage.shown = body.read_run ?? null;
    if (!o.last) keepRead("/data/today", body);
  };

  const todayWords = {
    words:
      "Asking the host: doctor, the fleet check, the incidents and the manual checks…",
    expect: 93,
  };

  /**
   * An earlier answer, on screen now; the table then shows the new run.
   * @param {any} body
   */
  const showLastToday = (body) => {
    items.ready({ refresh: false });
    paintToday(body, { last: true });
    // The last answer stays readable: the big layer is for a table with
    // nothing to show yet.
    items.loading({ ...todayWords, overlay: false });
  };

  /** @param {string} [url] a run's own url, for a `slow_read` event */
  const load = async (url) => {
    read.disabled = true;
    todayPage.reading = true;
    // A page this tab showed before paints at once; the dashboard's own
    // last answer follows within a request.
    const kept = url ? undefined : keptRead("/data/today");
    if (kept) showLastToday(kept);
    // The table's status line counts the seconds; the verdict's place
    // says what is being asked until the verdict replaces it.
    else items.loading(todayWords);
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
      r = await slowRead(url ?? "/data/today", "today", abort.signal, (b) =>
        showLastToday(b),
      );
    } finally {
      read.disabled = false;
      todayPage.reading = false;
    }
    if (!r.ok) {
      const took = humanDuration(items.failed(r.error));
      verdict.replaceChildren(
        h("p", { class: "measured", role: "status" }, `Failed after ${took}.`),
      );
      return;
    }
    const took = humanDuration(items.ready({ refresh: false }));
    paintToday(r.body, { took });
  };

  /**
   * Show one answer of /data/fleet-check.
   * @param {any} body
   * @param {{took?: string, last?: boolean}} o
   */
  const paintCheck = (body, o = {}) => {
    findings.wrap.hidden = false;
    const rows = findingRows(body.findings ?? []);
    findings.tbody.replaceChildren(
      ...rows.map((f) =>
        h(
          "tr",
          null,
          badgeCell(f.badge),
          f.diffable && f.driftStack
            ? driftDiffCell(f.driftStack)
            : td(f.subject),
          td(f.what),
          remedyCell(f.remedy, f.fix),
        ),
      ),
    );
    checkTable?.refresh();
    // How long ago is `checkAgo`'s own job, beside the title, same as
    // Today's — not repeated here as "checked in Xs" too.
    const when = o.last
      ? ", the last reading, while the host checks again"
      : "";
    checkNote.textContent =
      `${body.passes ? "Passes" : "Does not pass"}: ${rows.length} finding(s) over ${body.stack_files} stack file(s)${when}. ${body.skipped ?? ""} ${body.not_here ?? ""}`.trim();
    setAgo(checkAgo, body.read_at ?? body.measured_at ?? Date.now() / 1000);
    checkPage.shown = body.read_run ?? null;
    if (!o.last) keepRead("/data/fleet-check", body);
  };

  const checkWords = {
    words: "Running the fleet check on the host…",
    expect: 92,
  };

  /** @param {string} [url] a run's own url, for a `slow_read` event */
  const runCheck = async (url) => {
    checkBtn.disabled = true;
    checkPage.reading = true;
    findings.wrap.hidden = false;
    findings.loading(checkWords);
    checkNote.textContent = "The fleet check is running on the host.";
    let r;
    try {
      r = await slowRead(
        url ?? "/data/fleet-check",
        "the fleet check",
        abort.signal,
        (b) => {
          findings.ready({ refresh: false });
          paintCheck(b, { last: true });
          findings.loading({ ...checkWords, overlay: false });
        },
      );
    } finally {
      checkBtn.disabled = false;
      checkPage.reading = false;
    }
    if (!r.ok) {
      const took = humanDuration(findings.failed(r.error));
      checkNote.textContent = `The fleet check did not answer (after ${took}).`;
      return;
    }
    const took = humanDuration(findings.ready({ refresh: false }));
    paintCheck(r.body, { took });
  };

  // A run finished (this tab's, another tab's): fetch it by id, which
  // starts nothing on the host.
  const unlisten = listen("slow_read", (ev) => {
    if (fetchAnnounced(ev, "today", todayPage))
      void load(runUrl("/data/today", ev.run)).catch(() => {});
    if (fetchAnnounced(ev, "fleet-check", checkPage))
      void runCheck(runUrl("/data/fleet-check", ev.run)).catch(() => {});
  });
  // The fleet check runs only when asked (Today already carries it); a
  // reading this tab saw before is shown, with its age.
  const keptCheck = keptRead("/data/fleet-check");
  if (keptCheck) {
    findings.ready({ refresh: false });
    paintCheck(keptCheck);
  }

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
    unlisten();
    root.removeEventListener("kp-datatable-retry", retry);
    unbind();
    unbindC();
    detach();
  };
}
