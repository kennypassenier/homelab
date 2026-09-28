// feat-overview-5, feat-ops-8, feat-ops-9: the notification centre's pure
// half. The notices, the bell's count, the snooze in words and the per-stack
// switches, from the server's shapes; the clock and the locale come in.

import { formatTime, humanDuration } from "./format.js";

/**
 * @typedef {"action_done" | "action_failed" | "action_deferred" |
 *   "schedule_missed" | "incident"} NoticeKind
 * @typedef {{state: "sent"} | {state: "failed", why: string} |
 *   {state: "skipped", why: string}} Push
 * @typedef {{id: number, at: number, kind: NoticeKind, stack?: string,
 *   title: string, body: string, job?: number, read: boolean,
 *   push: Push}} Notice
 * @typedef {{push: boolean, muted_stacks: string[],
 *   snooze_until?: number | null, long_action_s: number}} NotifySettings
 * @typedef {{notices: Notice[], unread: number,
 *   unread_by_stack: Record<string, number>, settings: NotifySettings,
 *   snoozed: boolean}} NotifySnapshot
 * @typedef {import("./format.js").TimeOptions} TimeOptions
 */

/** How long a snooze can be picked for, in minutes (the server takes 0 to 10080). */
export const SNOOZE_CHOICES = /** @type {const} */ ([
  { minutes: 15, label: "15 minutes" },
  { minutes: 30, label: "30 minutes" },
  { minutes: 60, label: "1 hour" },
  { minutes: 120, label: "2 hours" },
  { minutes: 240, label: "4 hours" },
  { minutes: 480, label: "8 hours" },
  { minutes: 1440, label: "1 day" },
  { minutes: 10080, label: "1 week" },
]);

/** @type {Record<NoticeKind, {label: string, tone: "ok" | "warn" | "bad" | "info"}>} */
const KINDS = {
  action_done: { label: "action done", tone: "ok" },
  action_failed: { label: "action failed", tone: "bad" },
  action_deferred: { label: "action deferred", tone: "warn" },
  schedule_missed: { label: "schedule missed", tone: "warn" },
  incident: { label: "incident", tone: "bad" },
};

/** @param {NoticeKind} k */
export const kindBadge = (k) => KINDS[k] ?? { label: String(k), tone: "info" };

/** The kind column's sort order. */
export const KIND_ORDER =
  "incident,action failed,schedule missed,action deferred,action done";

/**
 * What became of a notice's push, in words.
 * @param {Push} p
 */
export function pushText(p) {
  if (!p) return "—";
  if (p.state === "sent") return "sent";
  if (p.state === "failed") return `failed: ${p.why}`;
  return `not sent: ${p.why}`;
}

/**
 * The bell: its count ("9+" past nine) and what a screen reader hears.
 * @param {number} unread
 */
export function bell(unread) {
  const n = Math.max(0, Math.floor(unread || 0));
  return {
    count: n === 0 ? "" : n > 9 ? "9+" : String(n),
    label:
      n === 0 ? "Notifications, none unread" : `Notifications, ${n} unread`,
  };
}

/**
 * Whether the snooze holds now, and until when in words.
 * @param {NotifySettings | null | undefined} s
 * @param {number} now unix seconds
 * @param {TimeOptions} [opts]
 * @returns {{on: boolean, text: string}}
 */
export function snoozeState(s, now, opts) {
  const until = s?.snooze_until;
  if (until == null || until <= now) return { on: false, text: "Not snoozed" };
  return {
    on: true,
    text: `Snoozed until ${formatTime(until, opts)} (${humanDuration(until - now)} from now)`,
  };
}

/**
 * The notices table's rows.
 * @param {Notice[]} notices
 */
export function noticeRows(notices) {
  return notices.map((n) => ({
    id: n.id,
    at: n.at,
    kind: kindBadge(n.kind),
    stack: n.stack ?? "—",
    title: n.title,
    body: n.body,
    push: pushText(n.push),
    read: n.read ? "read" : "unread",
    job: n.job ?? null,
  }));
}

/**
 * One row per stack, with its unread count and its mute switch.
 * @param {string[]} stacks every stack name the fleet knows
 * @param {NotifySnapshot | null} snap
 */
export function stackMuteRows(stacks, snap) {
  const muted = new Set(snap?.settings.muted_stacks ?? []);
  const unread = snap?.unread_by_stack ?? {};
  // A muted stack the fleet no longer lists still gets its row, so it can
  // be unmuted.
  const names = [...new Set([...stacks, ...muted])].sort();
  return names.map((s) => ({
    stack: s,
    unread: unread[s] ?? 0,
    muted: muted.has(s),
  }));
}

/**
 * A new notice laid into the snapshot (the `notification` event).
 * @param {NotifySnapshot | null} snap
 * @param {{notice: Notice, unread: number}} ev
 * @returns {NotifySnapshot | null}
 */
export function addNotice(snap, ev) {
  if (!snap) return snap;
  const rest = snap.notices.filter((n) => n.id !== ev.notice.id);
  const byStack = { ...snap.unread_by_stack };
  const s = ev.notice.stack;
  if (s && !ev.notice.read) byStack[s] = (byStack[s] ?? 0) + 1;
  return {
    ...snap,
    notices: [ev.notice, ...rest],
    unread: ev.unread,
    unread_by_stack: byStack,
  };
}

/**
 * The toast a pop-up notice raises: its words and its tone.
 * @param {Notice} n
 */
export function toastOf(n) {
  const k = kindBadge(n.kind);
  const tone =
    k.tone === "bad"
      ? "error"
      : k.tone === "warn"
        ? "warning"
        : k.tone === "ok"
          ? "success"
          : "info";
  return { text: `${n.title}${n.body ? `: ${n.body}` : ""}`, tone };
}

/**
 * The settings body for PUT /data/notifications/settings with one change.
 * @param {NotifySettings} s
 * @param {Partial<NotifySettings>} change
 * @returns {NotifySettings}
 */
export function settingsBody(s, change) {
  const out = { ...s, ...change };
  /** @type {NotifySettings} */
  const body = {
    push: out.push,
    muted_stacks: out.muted_stacks,
    long_action_s: out.long_action_s,
  };
  if (out.snooze_until != null) body.snooze_until = out.snooze_until;
  return body;
}
