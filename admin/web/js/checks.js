// Pure view model for the manual checks page (feat-ops-3). A record is the
// host's ManualCheckRecord (core/src/state.rs); fields this page does not
// know yet are still shown, generically, in the last column.

import { formatDateTime } from "./format.js";

/**
 * @typedef {{stack: string, app: string, text: string, registered_at: number,
 *   answered_at?: number | null, ok?: boolean | null, note?: string,
 *   answered_hash?: string | null, accepted_until?: number | null,
 *   [key: string]: unknown}} CheckRecord
 * @typedef {{id: string, record: CheckRecord}} Check
 * @typedef {{label: string, tone: "ok" | "warn" | "bad"}} Answer
 */

const KNOWN = new Set([
  "stack",
  "app",
  "text",
  "registered_at",
  "answered_at",
  "ok",
  "note",
  "accepted_until",
]);

/**
 * The answer as it stands at `now`: open until answered; a "not ok" that
 * is accepted until a later moment is noted, not broken.
 * @param {CheckRecord} r
 * @param {number} now unix seconds
 * @returns {Answer}
 */
export function checkAnswer(r, now) {
  if (r.ok == null) return { label: "open", tone: "warn" };
  if (r.ok) return { label: "ok", tone: "ok" };
  if (r.accepted_until != null && r.accepted_until > now)
    return { label: "accepted", tone: "warn" };
  return { label: "not ok", tone: "bad" };
}

/**
 * One unknown field as "name: value"; a unix moment (`*_at`, `*_until`)
 * reads as a time.
 * @param {string} key
 * @param {unknown} value
 * @param {import("./format.js").TimeOptions} [opts]
 */
export function extraField(key, value, opts) {
  const label = key.replaceAll("_", " ");
  if (/(_at|_until)$/.test(key) && typeof value === "number")
    return `${label}: ${formatDateTime(value, opts)}`;
  if (value !== null && typeof value === "object")
    return `${label}: ${JSON.stringify(value)}`;
  return `${label}: ${String(value)}`;
}

/**
 * The checks table's rows: open ones first, then by stack and app.
 * @param {Check[]} checks
 * @param {number} now unix seconds
 * @param {import("./format.js").TimeOptions} [opts]
 */
export function checkRows(checks, now, opts) {
  const rank = { "not ok": 0, open: 1, accepted: 2, ok: 3 };
  return checks
    .map(({ id, record: r }) => {
      const answer = checkAnswer(r, now);
      const note = [
        r.note ?? "",
        answer.label === "accepted" && r.accepted_until != null
          ? `accepted until ${formatDateTime(r.accepted_until, opts)}`
          : "",
      ]
        .filter((s) => s !== "")
        .join(" · ");
      const extras = Object.entries(r)
        .filter(([k, v]) => !KNOWN.has(k) && v != null && v !== "")
        .map(([k, v]) => extraField(k, v, opts));
      return {
        id,
        stack: r.stack,
        app: r.app,
        text: r.text,
        answer,
        registered: r.registered_at,
        answered: r.answered_at ?? null,
        note,
        extras: [`id: ${id}`, ...extras].join(" · "),
      };
    })
    .sort(
      (a, b) =>
        rank[/** @type {keyof typeof rank} */ (a.answer.label)] -
          rank[/** @type {keyof typeof rank} */ (b.answer.label)] ||
        a.stack.localeCompare(b.stack) ||
        a.app.localeCompare(b.app),
    );
}

/**
 * fix-checks-refresh: the answer jobs that finished well since the page
 * last looked, whoever pressed them (this tab, another, Claude, a
 * schedule); each is news once. `seen` holds the jobs already counted and
 * is updated. A failed answer changed nothing on the host.
 * @param {{job: number, action: string, state: string}[]} jobs
 * @param {Set<number>} seen
 * @returns {number[]}
 */
export function answeredJobs(jobs, seen) {
  const out = [];
  for (const j of jobs)
    if (j.action === "answer-check" && j.state === "done" && !seen.has(j.job)) {
      seen.add(j.job);
      out.push(j.job);
    }
  return out;
}
