// inbox-resolved-clears-1 (Kenny, 2026-10-23: "waarom zie ik dan 62
// meldingen in Inbox?" while "needs you" said 14): a notice whose cause is
// already gone is not an Inbox row. A later event on the same subject that
// says it is resolved settles it: "Sonarr answers again" settles "Sonarr
// does not answer", a good backup settles the failed one, a nightly check
// that no longer finds it supersedes the one that did. The host's own store
// settles what it can at the moment the news arrives
// (core/notify.rs `settles`, notice_store.rs `add`); this reads the same
// rule over the notices as the page holds them, so rows that were already
// open before the rule existed clear too. Pure: notices in, ids out.

/**
 * @typedef {import("./notices.js").Notice & {op?: string, label?: string,
 *   resolved_by?: number}} Settleable
 */

/** Kinds that report that something is fine again. */
const RESOLVING = new Set(["action_done", "up", "alert_resolved"]);
/** Kinds that report something wrong, and can be settled. */
const SETTLEABLE = new Set([
  "action_failed",
  "incident",
  "schedule_missed",
  "down",
  "alert",
  "fleet_check",
  "host_event",
]);

/** Which kinds of resolution end which kind of trouble. */
const ENDED_BY = /** @type {Record<string, string[]>} */ ({
  down: ["up"],
  alert: ["alert_resolved"],
  action_failed: ["action_done", "host_event"],
  schedule_missed: ["action_done", "host_event"],
  incident: ["action_done", "host_event"],
  host_event: ["action_done", "host_event"],
});

/**
 * Whether a notice reports that something is fine (or done): a success, an
 * answer again, a resolved alert, or any notice of level ok.
 * @param {Settleable} n
 */
export const isResolution = (n) => RESOLVING.has(n.kind) || n.level === "ok";

/** @param {string | undefined} op the operation, a schedule's prefix taken off */
const bare = (op) => (op ? op.replace(/^schedule-/, "") : "");

/**
 * What a notice is about, in words both the trouble and its end share: the
 * title without its outcome ("Back up media: failed" / "Back up media:
 * done", "scheduled-backup media failed" / "scheduled-backup media: done",
 * "Sonarr does not answer" / "Sonarr answers again", "Resolved: X" / "X").
 * @param {Settleable} n
 */
export function subjectOf(n) {
  return String(n.title ?? "")
    .replace(/^Resolved:\s*/i, "")
    .replace(/\s+(does not answer|answers again)$/i, "")
    .replace(
      /:?\s+(done|failed|refused|stood aside|outcome unknown|ok|resolved)$/i,
      "",
    )
    .trim()
    .toLowerCase();
}

/**
 * Whether the resolution `y` is about the same thing as the trouble `x`:
 * the same key, else the same stack and the same operation (or kind of
 * operation, or subject in words).
 * @param {Settleable} x
 * @param {Settleable} y
 */
function sameSubject(x, y) {
  if (x.key && y.key) return x.key === y.key;
  if ((x.stack ?? "") !== (y.stack ?? "")) return false;
  if (x.op && y.op && bare(x.op) === bare(y.op)) return true;
  if (x.stack && x.label && y.label && x.label === y.label) return true;
  const a = subjectOf(x);
  return a !== "" && a === subjectOf(y);
}

/**
 * The notices a later event settled: trouble `x` mapped to the notice `y`
 * that resolved it. A notice the host already settled (`resolved_by`) is
 * included. A nightly check is superseded by a newer nightly check of the
 * same stack.
 * @param {Settleable[]} notices
 * @returns {Map<number, number>}
 */
export function settledNotices(notices) {
  /** @type {Map<number, number>} */
  const out = new Map();
  const later = (/** @type {Settleable} */ x, /** @type {Settleable} */ y) =>
    y.id !== x.id && (y.at > x.at || (y.at === x.at && y.id > x.id));
  for (const x of notices) {
    if (x.resolved_by != null) {
      out.set(x.id, x.resolved_by);
      continue;
    }
    if (x.level === "ok" || !SETTLEABLE.has(x.kind) || isResolution(x))
      continue;
    // The newest resolution after it wins (any later one ends the trouble).
    let by = null;
    for (const y of notices) {
      if (!later(x, y)) continue;
      const ends =
        x.kind === "fleet_check"
          ? y.kind === "fleet_check" && (y.stack ?? "") === (x.stack ?? "")
          : isResolution(y) &&
            (ENDED_BY[x.kind] ?? []).includes(y.kind) &&
            sameSubject(x, y);
      if (ends && (!by || later(by, y))) by = y;
    }
    if (by) out.set(x.id, by.id);
  }
  return out;
}
