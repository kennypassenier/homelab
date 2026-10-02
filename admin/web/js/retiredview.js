// feat-retired-1: pure view-model for the Retired page. The host's
// `GetRetired` answers with one entry per `HostState::retired` key
// (`core::ops::retired::RetiredRow`, as JSON) — a stack, app or native
// unit that left the files, what it kept, and what a wipe of it would
// still leave behind because a managed stack uses it (ask-9's `in_use`).
// This module turns one entry into exactly the numbers and labels the
// table and the Wipe button need, so `pages/retired.js` only builds DOM
// from values already decided here. No DOM, no fetch, no clock.

/**
 * @typedef {{name: string, in_use: boolean, snapshot_count: number | null,
 *   size_bytes: number | null,
 *   newest_snapshot: {short_id: string, time: number} | null,
 *   measured_at: number | null}} RetiredRepo
 * @typedef {{key: string, kind: "stack" | "app" | "unit", stack: string,
 *   name: string, vmid: number, retired_at: number, repos: RetiredRepo[],
 *   appdata: string[], vault: string[], in_use: string[],
 *   refused: string | null}} RetiredEntry
 */

/**
 * What left the files, in the same three shapes `ops::retired` records
 * (`RetiredKind::Stack/App/Unit`) — the "by which operation" a row names.
 * An unknown kind (a host ahead of this dashboard) is shown as-is rather
 * than hidden or thrown on.
 * @param {string} kind
 */
export function operationLabel(kind) {
  switch (kind) {
    case "stack":
      return "stack destroyed or forgotten";
    case "app":
      return "app dropped from its stack";
    case "unit":
      return "native unit dropped from its stack";
    default:
      return kind;
  }
}

/**
 * The restic side of one entry folded into the numbers a table cell
 * wants: how many repositories, their snapshots and combined size, the
 * newest snapshot time across all of them, and how many have never been
 * read by the host's snapshot cache yet (fix-180: `measured_at` null
 * means "not read yet", never "zero").
 * @param {RetiredRepo[]} repos
 */
export function repoTotals(repos) {
  let snapshotCount = 0;
  let sizeBytes = 0;
  /** @type {number | null} */
  let newestTime = null;
  let unread = 0;
  for (const r of repos) {
    snapshotCount += r.snapshot_count ?? 0;
    sizeBytes += r.size_bytes ?? 0;
    if (r.measured_at == null) unread += 1;
    const t = r.newest_snapshot?.time;
    if (t != null && (newestTime == null || t > newestTime)) newestTime = t;
  }
  return {
    repoCount: repos.length,
    snapshotCount,
    sizeBytes,
    newestTime,
    unread,
  };
}

/**
 * Which of `paths` (the entry's `/appdata` directories, or its vault
 * copies) a managed stack still uses, per `inUse` (D25: an app that moved
 * stacks keeps its repository and its directory) — a wipe deletes
 * `removed` and leaves `kept` alone.
 * @param {string[]} paths
 * @param {string[]} inUse
 */
export function splitInUse(paths, inUse) {
  const used = new Set(inUse);
  return {
    kept: paths.filter((p) => used.has(p)),
    removed: paths.filter((p) => !used.has(p)),
  };
}

/**
 * What a wipe of this entry would do, read off `refused`/`in_use` exactly
 * as the host's own `wipe_plan` decided them — never re-derived from
 * `kind` or guessed in JavaScript:
 *
 * - `"refused"`: `wipe_plan` refuses the WHOLE key outright (`refused` is
 *   its reason) — the stack, app or native unit is declared again in the
 *   current state (live-finding 2026-10-02: a deploy bug wrongly retired a
 *   still-live native unit; `core::ops::retired::wipe_plan` now catches
 *   this through `StackState::natives`, not only a compose manifest's
 *   declared natives). The page shows this instead of a Wipe button —
 *   nothing of this record would be deleted by wiping it, so offering the
 *   button would only teach "Wipe" means "try and see".
 * - `"partial"`: wipe proceeds but keeps some repositories/paths because a
 *   DIFFERENT managed stack still uses them (D25).
 * - `"removable"`: wipe would delete everything this record kept.
 * @param {Pick<RetiredEntry, "in_use" | "refused">} entry
 */
export function wipeStatus(entry) {
  if (entry.refused) return "refused";
  return entry.in_use.length === 0 ? "removable" : "partial";
}
