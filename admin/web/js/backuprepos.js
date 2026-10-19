// feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups demo,
// "The repositories: one grouped, expandable table"): the markup of the
// repositories datatable — a heading row per stack (idmark, name, CT, count,
// Restore…), a row per repository (tick, name, stack, last 7 nights, snapshots,
// size bar, restore drill, Restore…), and the snapshots under an opened row.
// The datatable, the uptime strip, the size bar, the facets, the stack bar and
// the selection bar are the package's; this only writes their markup and the
// words. Pure of the page's state: the caller hands in what it has read.

import { h } from "./dom.js";
import { viaForm } from "./drivable.js";
import { formatDateTime } from "./format.js";
import { addDays, drillState, drillWords, humanBytes } from "./backupsview.js";
import { repoPeriods, nightDate, sizeText } from "./backupgrid.js";
import { stackHref } from "./router.js";
import { datedAge } from "/static/kp/js/dated-options.js";

/** The table's columns as the datatable counts them: the open arrow it prepends is 0, the ticks 1 (kp-gaps/homelab.md: a filter counts that control column). */
export const COL = /** @type {const} */ ({
  repo: 2,
  stack: 3,
  nights: 4,
  snaps: 5,
  size: 6,
  drill: 7,
  actions: 8,
});

/** Restore drill words, in the order a sort puts them: failed first. */
export const DRILL_ORDER = "failed,never drilled,passed";

const DRILL_TONE = /** @type {const} */ ({
  failed: "destructive",
  never: "warning",
  passed: "success",
});

/**
 * @typedef {import("./backupsview.js").StackRead} StackRead
 * @typedef {import("./backupsview.js").Repo} Repo
 * @typedef {{stack: string, native: boolean, repo: Repo}} RepoRef
 */

/**
 * The datatable around the repositories, empty and loading: its search, its
 * failed and empty slots, its status line. Rows are put into `tbody` later.
 * @param {string} id
 * @param {string} filterId the search box's id (a declared field)
 * @returns {{wrap: HTMLElement, tbody: HTMLElement}}
 */
export function reposShell(id, filterId) {
  const th = (
    /** @type {string} */ label,
    /** @type {Record<string, string>} */ a = {},
  ) => h("th", { scope: "col", ...a }, label);
  const tbody = h("tbody");
  const wrap = h(
    "div",
    {
      class: "kp-datatable bk-table",
      id,
      "data-kp-datatable": "",
      "data-kp-cards": "",
      "data-kp-expandable": "",
      "data-kp-page-size": "100",
      "data-kp-page-sizes": "none",
      "data-kp-max-height": "38rem",
      "data-kp-sort-multi": "",
      "data-kp-remember": "repos",
      "data-kp-busy-overlay": "",
      "data-kp-state": "loading",
    },
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("input", {
        class: "kp-datatable__search",
        type: "search",
        "data-kp-datatable-search": "",
        placeholder: "Search the repositories",
        "aria-label": "Search the repositories (press / to jump here)",
        id: filterId,
        name: "bk-filter",
        autocomplete: "off",
      }),
    ),
    h(
      "div",
      {
        class: "kp-alert kp-alert--destructive",
        "data-kp-datatable-failed": "",
        role: "alert",
        hidden: "",
      },
      h(
        "p",
        { class: "kp-alert__label kp-stateplate__title" },
        "The repositories could not be read",
      ),
      h("p", { "data-kp-datatable-failed-reason": "" }),
      h(
        "button",
        {
          type: "button",
          class: "kp-button",
          "data-kp-datatable-retry": "",
        },
        "Try again",
      ),
    ),
    h(
      "div",
      { class: "kp-table-wrap" },
      h(
        "table",
        { class: "kp-table" },
        h(
          "caption",
          { class: "kp-sr-only" },
          "Backup repositories, grouped by stack",
        ),
        h(
          "thead",
          null,
          h(
            "tr",
            null,
            h(
              "th",
              { scope: "col" },
              h("input", {
                class: "kp-field__check",
                type: "checkbox",
                "data-kp-select-all": "",
                "aria-label": "Select all repositories",
              }),
            ),
            th("Repository", { "data-kp-sort": "text" }),
            th("Stack", { "data-kp-sort": "text", "data-kp-filter": "choice" }),
            th("Last 7 nights", { "data-kp-sort": "number" }),
            th("Snapshots", {
              "data-kp-sort": "number",
              class: "kp-text-end",
            }),
            th("Size", { "data-kp-sort": "number" }),
            th("Restore drill", {
              "data-kp-sort": "text",
              "data-kp-filter": "choice",
              "data-kp-sort-order": DRILL_ORDER,
            }),
            th("Actions"),
          ),
        ),
        tbody,
      ),
    ),
    h(
      "div",
      { class: "kp-empty", "data-kp-datatable-empty": "", hidden: "" },
      h(
        "div",
        { "data-kp-datatable-empty-none": "" },
        h("p", { class: "kp-empty__title" }, "No repositories yet"),
        h(
          "p",
          null,
          "The first nightly round creates one per app of every stack that keeps data.",
        ),
      ),
      h(
        "div",
        { "data-kp-datatable-empty-nomatch": "", hidden: "" },
        h("p", { class: "kp-empty__title" }, "No repository matches"),
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--secondary",
            "data-kp-datatable-clear": "",
          },
          "Clear the search and filters",
        ),
      ),
    ),
    h(
      "div",
      { class: "kp-datatable__bar" },
      h("p", {
        class: "kp-datatable__status",
        "data-kp-datatable-status": "",
        role: "status",
        "aria-live": "polite",
      }),
      h("div", { class: "kp-datatable__pager", "data-kp-datatable-pager": "" }),
    ),
  );
  return { wrap, tbody };
}

/**
 * The selection bar that belongs to the table: it reads the ticked rows and
 * names them; Verify restore and Back up now are the page's.
 * @param {string} tableId
 */
export const selectionBar = (tableId) => {
  const verify = viaForm(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        "data-kp-selection-action": "verify",
        title:
          "Restore each ticked repository's newest snapshot into the drill's scratch directory to prove it restores; live data is not touched",
      },
      "Verify restore",
    ),
    "verify-restore",
  );
  const backup = viaForm(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        "data-kp-selection-action": "backup",
        title:
          "Run a backup now of the stacks the ticked repositories belong to",
      },
      "Back up now",
    ),
    "batch",
  );
  return h(
    "div",
    {
      class: "kp-selection",
      "data-kp-selection": "",
      "data-kp-selection-for": tableId,
      "data-kp-placement": "sticky-top",
      "aria-label": "Selected repositories",
    },
    h("div", { class: "kp-selection__actions" }, verify, backup),
  );
};

/** Why a drill reads the way it does, for the hover card. @param {Repo} r */
export function drillText(r) {
  const d = drillState(r);
  if (d === "never")
    return "This repository was never restored into scratch to prove it works.";
  if (d === "failed")
    return `Last drill ${formatDateTime(r.drill?.last_attempt)} failed: ${r.drill?.last_error}`;
  return `Last passed ${formatDateTime(r.drill?.last_pass)}.`;
}

/**
 * A heading row for one stack: the identity mark, the name (a link to the
 * stack), CT number, the count badge, what the stack holds when it has no
 * repositories, and Restore… (or Retry when the read failed).
 * @param {{stack: string, vmid: number | null, read: StackRead,
 *   total: number, restore: HTMLElement | null, why: string | null}} g
 */
export function groupRow(g) {
  const always = g.total === 0;
  return h(
    "tr",
    {
      "data-kp-row-group": g.stack,
      "data-kp-group-label": g.stack,
      "data-kp-group-always": always ? "" : null,
    },
    h(
      "th",
      { scope: "rowgroup", colspan: "8" },
      h(
        "span",
        { class: "bk-group" },
        h("span", {
          class: "kp-idmark kp-idmark--pattern",
          "data-kp-idmark": g.stack,
          "aria-hidden": "true",
        }),
        h(
          "a",
          {
            class: "bk-group__name",
            href: stackHref(g.stack),
            title: `Open ${g.stack}'s page`,
          },
          g.stack,
        ),
        h(
          "span",
          { class: "bk-group__ct" },
          g.vmid != null ? `CT ${g.vmid}` : "",
        ),
        h("span", { class: "kp-badge", "data-kp-group-count": "" }),
        h("span", { class: "bk-group__why" }, g.why ?? ""),
        h("span", { class: "bk-group__act" }, g.restore),
      ),
    ),
  );
}

/**
 * A row for one repository.
 * @param {{stack: string, native: boolean, repo: Repo, read: StackRead,
 *   now: {last: string, current: string}, max: number,
 *   restore: HTMLElement}} x
 */
export function repoRow(x) {
  const { repo: r } = x;
  const key = `${x.stack}/${r.owner}`;
  const d = drillState(r);
  const periods = x.read.status === "ok" ? repoPeriods(x.read, r, x.now) : [];
  const up = periods.filter((p) => p.state === "up").length;
  const bytes = r.size_bytes;
  return h(
    "tr",
    { "data-kp-row-key": key, "data-kp-group-of": x.stack },
    h(
      "td",
      { "data-label": "Select" },
      h("input", {
        class: "kp-field__check",
        type: "checkbox",
        "data-kp-select-row": "",
        "aria-label": `Select ${x.stack} ${r.owner}`,
      }),
    ),
    h(
      "td",
      { "data-label": "Repository" },
      h("code", { class: "kp-mono" }, r.owner),
      r.error && !r.newest_snapshot
        ? h("span", { class: "bk-hint bk-repo__note" }, r.error)
        : null,
    ),
    h("td", { "data-label": "Stack" }, x.stack),
    h(
      "td",
      { "data-label": "Last 7 nights", "data-kp-sort-value": String(up) },
      h("div", {
        class: "kp-uptime",
        "data-kp-uptime": "",
        "data-kp-uptime-count": "7",
        "data-bk-uptime": key,
      }),
    ),
    h(
      "td",
      {
        "data-label": "Snapshots",
        class: "kp-text-end kp-numeric",
        "data-kp-sort-value": String(r.snapshot_count ?? 0),
      },
      String((r.snapshot_count ?? 0).toLocaleString("en-GB")),
    ),
    h(
      "td",
      {
        "data-label": "Size",
        "data-kp-sort-value": String(bytes ?? -1),
      },
      h("span", {
        class: "kp-bar-cell",
        "data-kp-bar": "",
        "data-bk-size": String(bytes ?? ""),
        "data-bk-size-max": String(x.max),
      }),
    ),
    h(
      "td",
      { "data-label": "Restore drill" },
      h(
        "span",
        {
          class: `kp-badge kp-badge--${DRILL_TONE[d]}`,
          tabindex: "0",
          "data-kp-hovercard": drillText(r),
        },
        drillWords(d),
      ),
    ),
    h(
      "td",
      { "data-label": "Actions" },
      h("div", { class: "kp-row-actions" }, x.restore),
    ),
  );
}

/**
 * Feed the uptime strips and the size bars of the rows now in `root`.
 * @param {ParentNode} root
 * @param {Map<string, {read: StackRead, repo: Repo}>} refs
 * @param {{last: string, current: string}} now
 * @param {(el: HTMLElement, spec: import("/static/kp/js/uptime.js").UptimeSpec) => void} setUptime
 * @param {(el: HTMLElement, spec: import("/static/kp/js/barcell.js").BarCell) => void} setBarCell
 */
export function feedCells(root, refs, now, setUptime, setBarCell) {
  for (const el of /** @type {HTMLElement[]} */ ([
    ...root.querySelectorAll("[data-bk-uptime]"),
  ])) {
    const ref = refs.get(el.getAttribute("data-bk-uptime") ?? "");
    if (!ref || ref.read.status !== "ok") continue;
    setUptime(el, {
      periods: repoPeriods(ref.read, ref.repo, now),
      from: nightDate(addDays(now.last, -6)),
      to: "Last night",
      count: 7,
      open: "info",
    });
  }
  for (const el of /** @type {HTMLElement[]} */ ([
    ...root.querySelectorAll("[data-bk-size]"),
  ])) {
    const raw = el.getAttribute("data-bk-size");
    const max = Number(el.getAttribute("data-bk-size-max")) || 1;
    const bytes = raw === "" || raw == null ? null : Number(raw);
    if (bytes == null) {
      setBarCell(el, {
        value: null,
        text: "—",
        sortValue: -1,
        label: "The host could not measure this repository's size",
      });
    } else {
      setBarCell(el, {
        value: bytes / max,
        text: sizeText(bytes),
        sortValue: bytes,
        tone: null,
        label: `of the biggest repository, ${sizeText(max)}`,
      });
    }
  }
}

/**
 * The snapshots under an opened repository: id, taken, age, size, and
 * Restore… and Show a file… per snapshot in two fixed columns.
 * @param {{stack: string, native: boolean, repo: Repo}} ref
 * @param {number} nowMs
 * @param {(snap: import("./backupsview.js").Snap) => HTMLElement[]} actions
 */
export function snapshotList(ref, nowMs, actions) {
  const snaps = ref.repo.snapshots ?? [];
  const shown = snaps.slice(0, 6);
  return h(
    "div",
    { class: "bk-snaps" },
    h(
      "table",
      { class: "kp-table bk-snaps__table" },
      h(
        "caption",
        { class: "kp-sr-only" },
        `Snapshots of ${ref.stack} ${ref.repo.owner}`,
      ),
      h(
        "thead",
        null,
        h(
          "tr",
          null,
          h("th", { scope: "col" }, "Snapshot"),
          h("th", { scope: "col" }, "Taken"),
          h("th", { scope: "col" }, "Age"),
          h("th", { scope: "col", class: "kp-text-end" }, "Size"),
          h("th", { scope: "col" }, "Actions"),
        ),
      ),
      h(
        "tbody",
        null,
        ...shown.map((x) =>
          h(
            "tr",
            null,
            h(
              "td",
              null,
              h("code", { class: "kp-mono", title: x.id }, x.short_id),
            ),
            h("td", { class: "kp-numeric" }, formatDateTime(x.time)),
            h("td", null, datedAge(x.time * 1000, nowMs)),
            h(
              "td",
              { class: "kp-text-end kp-numeric" },
              humanBytes(/** @type {any} */ (x).size_bytes) ?? "—",
            ),
            h("td", null, h("div", { class: "kp-row-actions" }, ...actions(x))),
          ),
        ),
      ),
    ),
    h(
      "p",
      { class: "bk-hint" },
      snaps.length > shown.length
        ? `The ${shown.length} newest of ${snaps.length.toLocaleString("en-GB")} snapshots.`
        : `${snaps.length.toLocaleString("en-GB")} ${snaps.length === 1 ? "snapshot" : "snapshots"}, newest first.`,
    ),
  );
}
