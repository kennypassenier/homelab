// feat-backup-1/2/3: the Backups page — every stack's restic repositories
// (D25: one per owning app for a compose stack, one per unit for a native
// stack), the newest snapshot, its age and size, and the last restore-drill
// verdict recorded for that repository. Restore picks a snapshot here and
// opens the existing Restore/Restore (native) action dialog with it
// pre-filled; browsing a snapshot expands its file list inline, read-only.

import {
  badgeCell,
  fetchJson,
  h,
  progressGroup,
  tableBlock,
  td,
} from "../dom.js";
import { agoText, humanDuration } from "../format.js";
import { humanMb } from "../fleet.js";
import { openAction } from "../actiondialog.js";
import { stackReadProgress, withStackResult } from "../perstack.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

const NOTHING = "No stack has a backup repository yet.";

/**
 * fix-179: `ok`, with the raw fields `rows` builds a repository table from;
 * `failed`: the read itself did not finish — named under the progress bar
 * and never folded into "this stack has no repository" the way a plain
 * `if (r.ok) out.push(...)` used to.
 * @typedef {{status: "pending"} | {status: "ok", native: boolean,
 *   repos: any[]} | {status: "failed", reason: string}} StackResult
 */

/**
 * @param {number | null | undefined} unixSeconds
 */
function age(unixSeconds) {
  if (unixSeconds == null) return "—";
  return humanDuration(Date.now() / 1000 - unixSeconds);
}

/**
 * @param {number | null | undefined} unixSeconds
 */
function when(unixSeconds) {
  if (!unixSeconds) return "never";
  return new Date(unixSeconds * 1000).toLocaleString();
}

/**
 * @param {string} stack
 * @param {boolean} native
 * @param {any[]} repos
 */
function rows(stack, native, repos) {
  return repos.map((r) => {
    const drillTone = !r.drill
      ? "neutral"
      : r.drill.last_error
        ? "bad"
        : r.drill.last_pass >= r.drill.last_attempt
          ? "good"
          : "warn";
    const drillLabel = !r.drill
      ? "never drilled"
      : r.drill.last_error
        ? `failed: ${r.drill.last_error}`
        : `passed ${when(r.drill.last_pass)}`;
    const tr = h(
      "tr",
      { "data-kp-row-key": `${stack}-${r.owner}` },
      h("td", null, h("a", { href: stackHref(stack) }, stack)),
      td(r.owner, "mono"),
      td(
        r.error
          ? `error: ${r.error}`
          : r.newest_snapshot
            ? r.newest_snapshot.short_id
            : "—",
      ),
      td(age(r.newest_snapshot?.time)),
      td(humanMb((r.size_bytes ?? 0) / (1024 * 1024))),
      td(String(r.snapshot_count)),
      // fix-180: the host answers from its own snapshot cache now, so a
      // repository's row can be older than the page load — "read N min
      // ago" says how old, same wording `agoText` already gives Today and
      // the fleet check.
      td(agoText("read", r.measured_at, Math.floor(Date.now() / 1000))),
      badgeCell({ label: drillLabel, tone: drillTone }),
      h("td", null, ...restoreCell(stack, native, r)),
    );
    return tr;
  });
}

/**
 * @param {string} stack
 * @param {boolean} native
 * @param {any} r
 * @returns {Node[]}
 */
function restoreCell(stack, native, r) {
  if (!r.newest_snapshot) return [document.createTextNode("—")];
  const btn = h(
    "button",
    {
      class: "kp-button kp-button--sm",
      type: "button",
      "data-action": native ? "restore-native" : "restore",
    },
    "Restore…",
  );
  btn.addEventListener(
    "click",
    () =>
      void openAction(stack, native ? "restore-native" : "restore", {
        preset: { snapshot: r.newest_snapshot.short_id },
      }),
  );
  return [btn];
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const table = tableBlock({
    remember: "backups",
    caption: "Every repository of every stack",
    search: "Search stacks or repositories",
    state: "loading",
    nothing: NOTHING,
    pageSize: 50,
    pageSizes: "25,50,100,250",
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "Repository", sort: "text" },
      { label: "Newest snapshot", sort: "text" },
      { label: "Age", sort: "text" },
      { label: "Size", sort: "text" },
      { label: "Snapshots", sort: "number" },
      { label: "Read", sort: "text" },
      { label: "Restore drill", sort: "text", filter: "choice" },
      { label: "", sort: "none" },
    ],
  });
  const progressWrap = h("div", { class: "backups__progress" });
  const failures = h("ul", {
    class: "backups__errors",
    "aria-live": "polite",
  });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Backups")),
    h(
      "p",
      null,
      "Each stack's restic repositories (one per app that keeps data; a native service has one of its own), read from the host. Pick Restore to choose a snapshot and bring the stack's data back to it.",
    ),
    progressWrap,
    failures,
    table.wrap,
  );
  const detach = attachDataTables(root);
  const abort = new AbortController();

  /**
   * The progress bar and the failed-stack list, redrawn from `results`
   * (fix-179: a stack that could not be read is named here, with its
   * reason, never silently dropped from the table the way an `if (r.ok)`
   * push once did).
   * @param {Record<string, StackResult>} results
   */
  const repaint = (results) => {
    const progress = stackReadProgress(results);
    progressWrap.replaceChildren(
      ...(progress.total
        ? [
            progressGroup([
              {
                label: "Stacks read",
                pct: progress.pct,
                value: `${progress.loaded} of ${progress.total}`,
              },
            ]),
          ]
        : []),
    );
    failures.replaceChildren(
      ...progress.failed.map((f) =>
        h(
          "li",
          { class: "kp-alert kp-alert--destructive" },
          h("strong", null, f.stack),
          `: could not be read — ${f.reason}`,
        ),
      ),
    );
    return progress;
  };

  const load = async () => {
    progressWrap.replaceChildren();
    failures.replaceChildren();
    const names = (current().fleet?.stacks ?? []).map((s) => s.name);
    if (names.length === 0) {
      table.setNothing(NOTHING);
      table.tbody.replaceChildren();
      table.ready();
      return;
    }
    table.loading({
      words: `Reading each stack's backup status from the host — 0 of ${names.length} so far…`,
    });

    /** @type {Record<string, StackResult>} */
    let results = Object.fromEntries(
      names.map((n) => [n, { status: "pending" }]),
    );
    repaint(results);

    await Promise.allSettled(
      names.map(async (name) => {
        /** @type {StackResult} */
        let outcome;
        try {
          const r = await fetchJson(
            `/data/backups/${encodeURIComponent(name)}`,
            `${name}'s backups`,
            abort.signal,
          );
          outcome = r.ok
            ? {
                status: "ok",
                native: r.body?.native === true,
                repos: r.body?.repos ?? [],
              }
            : {
                status: "failed",
                reason: r.error.fix
                  ? `${r.error.why} — ${r.error.fix}`
                  : r.error.why,
              };
        } catch (e) {
          if (abort.signal.aborted) return;
          outcome = { status: "failed", reason: String(e) };
        }
        if (abort.signal.aborted) return;
        results = withStackResult(results, name, outcome);
        const progress = repaint(results);
        if (!abort.signal.aborted)
          table.loading({
            words: `Reading each stack's backup status from the host — ${progress.loaded} of ${progress.total} so far…`,
          });
      }),
    );
    if (abort.signal.aborted) return;

    /** @type {Node[]} */
    const out = [];
    for (const name of names) {
      const r = results[name];
      if (r.status === "ok") out.push(...rows(name, r.native, r.repos));
    }
    const progress = repaint(results);
    table.setNothing(
      out.length === 0 && progress.failed.length > 0
        ? "No stack could be read — see the errors above."
        : NOTHING,
    );
    table.tbody.replaceChildren(...out);
    table.ready();
  };
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  const off = subscribe(retry);
  retry();
  return () => {
    abort.abort();
    off();
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
