// feat-backup-1/2/3: the Backups page — every stack's restic repositories
// (D25: one per owning app for a compose stack, one per unit for a native
// stack), the newest snapshot, its age and size, and the last restore-drill
// verdict recorded for that repository. Restore picks a snapshot here and
// opens the existing Restore/Restore (native) action dialog with it
// pre-filled; browsing a snapshot expands its file list inline, read-only.

import { badgeCell, fetchJson, h, tableBlock, td } from "../dom.js";
import { humanDuration } from "../format.js";
import { humanMb } from "../fleet.js";
import { openAction } from "../actiondialog.js";
import { stackHref } from "../router.js";
import { current, subscribe } from "../store.js";
import { attachDataTables, dataTable } from "/static/kp/js/datatable.js";

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
    nothing: "No stack has a backup repository yet.",
    pageSize: 50,
    pageSizes: "25,50,100,250",
    columns: [
      { label: "Stack", sort: "text", filter: "choice" },
      { label: "Repository", sort: "text" },
      { label: "Newest snapshot", sort: "text" },
      { label: "Age", sort: "text" },
      { label: "Size", sort: "text" },
      { label: "Snapshots", sort: "number" },
      { label: "Restore drill", sort: "text", filter: "choice" },
      { label: "", sort: "none" },
    ],
  });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Backups")),
    h(
      "p",
      null,
      "Each stack's restic repositories (one per app that keeps data; a native service has one of its own), read from the host. Pick Restore to choose a snapshot and bring the stack's data back to it.",
    ),
    table.wrap,
  );
  const detach = attachDataTables(root);
  const abort = new AbortController();

  const load = async () => {
    table.loading({
      words: "Reading each stack's backup status from the host…",
    });
    const stacks = current().fleet?.stacks ?? [];
    /** @type {Node[]} */
    const out = [];
    await Promise.all(
      stacks.map(async (s) => {
        const r = await fetchJson(
          `/data/backups/${encodeURIComponent(s.name)}`,
          `${s.name}'s backups`,
          abort.signal,
        );
        if (r.ok) {
          out.push(
            ...rows(s.name, r.body?.native === true, r.body?.repos ?? []),
          );
        }
      }),
    );
    if (out.length === 0) {
      table.loading({ words: "" });
    }
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
