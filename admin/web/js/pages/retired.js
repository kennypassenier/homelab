// feat-retired-1: the Retired page — every `HostState::retired` entry (a
// stack, app or native unit the host still KEEPS everything of, ask-9),
// what it keeps, and a "Wipe…" button that opens the existing wipe dialog
// (`ActionKind::Wipe`) on it.
//
// Kenny, 2026-10-02: "is there a way, from the admin dashboard, to delete
// backups of services we no longer use? That seems desirable." Before this
// page, `ActionKind::Wipe` was reachable only from the stack's own page (the
// "Retire" group next to Destroy/Forget) — unreachable the moment the stack
// itself is gone, which is exactly when a wipe is wanted. This page lists
// every retired key regardless of whether its stack still has a page, and
// `homelab ui open wipe <key>` (Live view) now opens the same dialog on a
// key that is not a live stack (`core::drive`'s `targets_retired`).
//
// It is its own page in the Configure group rather than a section of
// Backups (Kenny asked which reads better): Backups is built around "every
// managed stack's repositories", one row per owning app of a STACK THAT
// EXISTS; a retired entry has no stack page to link to and no live manifest
// to read apps/units from, so folding it into that per-stack loop would mean
// a second, different kind of row in the same table. A dedicated page keeps
// both models simple, and sits naturally beside Secrets/Settings/Firewall as
// "what the fleet's lifecycle looks like", not as a kind of backup status.

import { badgeCell, fetchJson, h, tableBlock, td } from "../dom.js";
import { humanDuration } from "../format.js";
import { humanMb } from "../fleet.js";
import { openAction } from "../actiondialog.js";
import {
  fullyRemovable,
  operationLabel,
  repoTotals,
  splitInUse,
} from "../retiredview.js";
import { attachDataTables } from "/static/kp/js/datatable.js";

const NOTHING = "Nothing retired is kept right now.";

/**
 * @param {number | null | undefined} unixSeconds
 */
function age(unixSeconds) {
  if (unixSeconds == null) return "—";
  return humanDuration(Date.now() / 1000 - unixSeconds);
}

/**
 * @param {ReturnType<typeof repoTotals>} totals
 */
function reposCell(totals) {
  if (totals.repoCount === 0) return "none";
  const parts = [
    `${totals.snapshotCount} snapshot(s)`,
    humanMb(totals.sizeBytes / (1024 * 1024)),
  ];
  if (totals.newestTime != null) {
    parts.push(
      `newest ${humanDuration(Date.now() / 1000 - totals.newestTime)} ago`,
    );
  }
  if (totals.unread > 0) parts.push(`${totals.unread} not read yet`);
  return parts.join(", ");
}

/**
 * @param {string[]} removed
 * @param {string[]} kept
 * @param {string} noun
 */
function pathsCell(removed, kept, noun) {
  if (removed.length === 0 && kept.length === 0) return "none";
  const parts = [];
  if (removed.length > 0) parts.push(`${removed.length} ${noun}`);
  if (kept.length > 0) parts.push(`${kept.length} kept (in use elsewhere)`);
  return parts.join(", ");
}

/**
 * @param {import("../retiredview.js").RetiredEntry} entry
 */
function row(entry) {
  const totals = repoTotals(entry.repos);
  const appdata = splitInUse(entry.appdata, entry.in_use);
  const vault = splitInUse(entry.vault, entry.in_use);
  const removable = fullyRemovable(entry);
  const tr = h(
    "tr",
    { "data-kp-row-key": entry.key },
    td(entry.kind, "mono"),
    h("td", null, h("strong", null, entry.key)),
    td(operationLabel(entry.kind)),
    td(age(entry.retired_at)),
    td(reposCell(totals)),
    td(pathsCell(appdata.removed, appdata.kept, "dir(s)")),
    td(pathsCell(vault.removed, vault.kept, "file(s)")),
    badgeCell(
      removable
        ? { label: "fully removable", tone: "good" }
        : { label: "partly kept in use", tone: "warn" },
    ),
    h("td", null, wipeButton(entry)),
  );
  return tr;
}

/**
 * @param {import("../retiredview.js").RetiredEntry} entry
 */
function wipeButton(entry) {
  const btn = h(
    "button",
    {
      class: "kp-button kp-button--sm kp-button--destructive",
      type: "button",
      "data-action": "wipe",
    },
    "Wipe…",
  );
  btn.addEventListener("click", () => void openAction(entry.key, "wipe"));
  return btn;
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const table = tableBlock({
    remember: "retired",
    caption:
      "Every stack, app or native unit the host keeps a retired record of",
    search: "Search retired keys",
    state: "loading",
    nothing: NOTHING,
    pageSize: 50,
    pageSizes: "25,50,100,250",
    columns: [
      { label: "Kind", sort: "text", filter: "choice" },
      { label: "Key", sort: "text" },
      { label: "Retired by", sort: "text", filter: "choice" },
      { label: "Age", sort: "text" },
      { label: "Restic", sort: "text" },
      { label: "/appdata", sort: "text" },
      { label: "Vault", sort: "text" },
      { label: "Kept", sort: "text", filter: "choice" },
      { label: "", sort: "none" },
    ],
  });
  root.replaceChildren(
    h("div", { class: "title-row" }, h("h1", null, "Retired")),
    h(
      "p",
      null,
      "Every stack, app or native unit a destroy, forget or deploy retired (ask-9): nothing here is deleted on its own. Wipe deletes exactly what one entry kept — its restic repositories, /appdata directories and vault copies — after you type its key to confirm; what a managed stack still uses is kept even then.",
    ),
    table.wrap,
  );
  const detach = attachDataTables(root);
  const abort = new AbortController();

  const load = async () => {
    table.loading({ words: "Reading the retired list from the host…" });
    const r = await fetchJson(
      "/data/retired",
      "the retired list",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) {
      table.failed(r.error);
      return;
    }
    const entries = r.body?.retired ?? [];
    table.tbody.replaceChildren(...entries.map(row));
    table.ready();
  };
  const retry = () => void load().catch(() => {});
  root.addEventListener("kp-datatable-retry", retry);
  retry();
  return () => {
    abort.abort();
    root.removeEventListener("kp-datatable-retry", retry);
    detach();
  };
}
