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
  perstackChips,
  tableBlock,
  td,
} from "../dom.js";
import { agoText, humanDuration } from "../format.js";
import { humanMb } from "../fleet.js";
import { openAction } from "../actiondialog.js";
import { stackChips, stackReadProgress, withStackResult } from "../perstack.js";
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
  const chipsWrap = h("div", { class: "backups__chips-wrap" });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Backups")),
    h(
      "p",
      null,
      "Each stack's restic repositories (one per app that keeps data; a native service has one of its own), read from the host. Pick Restore to choose a snapshot and bring the stack's data back to it.",
    ),
    chipsWrap,
    table.wrap,
  );
  const detach = attachDataTables(root);
  const abort = new AbortController();
  /** @type {Record<string, number>} */
  const startedAt = {};
  /** @type {Record<string, StackResult>} */
  let liveResults = {};

  /**
   * fix-216: a named chip grid, not a bare fraction — every stack's own
   * state, with a Retry on a failed one so a single hung repository never
   * needs the whole page reloaded.
   * @param {Record<string, StackResult>} results
   */
  const repaint = (results) => {
    liveResults = results;
    const progress = stackReadProgress(results);
    const chips = stackChips(results, startedAt, Date.now());
    chipsWrap.replaceChildren(
      perstackChips(chips, { onRetry: (name) => void retryOne(name) }),
    );
    return progress;
  };

  /**
   * One read of a single stack's repositories, shared by the initial
   * fleet-wide load and a chip's own Retry (fix-216).
   * @param {string} name
   * @returns {Promise<StackResult>}
   */
  const readOne = async (name) => {
    try {
      const r = await fetchJson(
        `/data/backups/${encodeURIComponent(name)}`,
        `${name}'s backups`,
        abort.signal,
      );
      return r.ok
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
      if (abort.signal.aborted) return { status: "pending" };
      return { status: "failed", reason: String(e) };
    }
  };

  /** Repaints the table from `results` — every "ok" stack's repositories. */
  const renderTable = (/** @type {Record<string, StackResult>} */ results) => {
    /** @type {Node[]} */
    const out = [];
    for (const [name, r] of Object.entries(results))
      if (r.status === "ok") out.push(...rows(name, r.native, r.repos));
    const progress = stackReadProgress(results);
    table.setNothing(
      out.length === 0 && progress.failed.length > 0
        ? "No stack could be read — see the chips above."
        : NOTHING,
    );
    table.tbody.replaceChildren(...out);
    table.ready();
  };

  /**
   * fix-216: one chip's own Retry, re-reading just that repository.
   * @param {string} name
   */
  async function retryOne(name) {
    if (abort.signal.aborted) return;
    liveResults = withStackResult(liveResults, name, { status: "pending" });
    repaint(liveResults);
    startedAt[name] = Date.now();
    const outcome = await readOne(name);
    if (abort.signal.aborted) return;
    liveResults = withStackResult(liveResults, name, outcome);
    repaint(liveResults);
    renderTable(liveResults);
  }

  const load = async () => {
    chipsWrap.replaceChildren();
    // fix-210: the fleet itself (`/data/fleet`, store.js) may not have
    // answered yet — that is "still loading", never "no stack has a
    // backup repository yet" (the same distinction overview.js already
    // makes with `if (!current().fleet) t.loading(...)`), so a slow first
    // read never paints as an empty table.
    if (!current().fleet) {
      table.loading({ words: "Waiting for the host's report of the fleet…" });
      return;
    }
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

    liveResults = Object.fromEntries(
      names.map((n) => [n, { status: /** @type {const} */ ("pending") }]),
    );
    for (const n of names) startedAt[n] = Date.now();
    repaint(liveResults);

    await Promise.allSettled(
      names.map(async (name) => {
        const outcome = await readOne(name);
        if (abort.signal.aborted) return;
        liveResults = withStackResult(liveResults, name, outcome);
        const progress = repaint(liveResults);
        if (!abort.signal.aborted)
          table.loading({
            words: `Reading each stack's backup status from the host — ${progress.loaded} of ${progress.total} so far…`,
          });
      }),
    );
    if (abort.signal.aborted) return;
    renderTable(liveResults);
  };
  // fix-204 (Kenny, 2026-10-02: "zes keer zfs … nu zeven, data moet maar
  // één keer laden"): the store notifies on every live update, and each
  // notice started a whole new read on top of the running one, so rows
  // piled up. Now one read at a time; a store notice only reloads when the
  // set of stacks itself changed, and a retry asked for during a read runs
  // once after it.
  let running = false;
  let again = false;
  let lastNames = "";
  const namesKey = () =>
    (current().fleet?.stacks ?? [])
      .map((s) => s.name)
      .sort()
      .join(",");
  const retry = () => {
    if (running) {
      again = true;
      return;
    }
    running = true;
    lastNames = namesKey();
    void load()
      .catch(() => {})
      .finally(() => {
        running = false;
        if (again && !abort.signal.aborted) {
          again = false;
          retry();
        }
      });
  };
  const onStore = () => {
    if (namesKey() !== lastNames) retry();
  };
  root.addEventListener("kp-datatable-retry", retry);
  const off = subscribe(onStore);
  retry();
  return () => {
    abort.abort();
    off();
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
