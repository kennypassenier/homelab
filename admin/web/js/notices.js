// feat-overview-5, feat-ops-8, feat-ops-9: the notification centre's pure
// half. The notices, the bell's count, the snooze in words and the per-stack
// switches, from the server's shapes; the clock and the locale come in.
// Decision "Notifications and Grafana" (2026-09-30): every notice says what,
// since when, the consequence and what to do, with its page and, when the
// dashboard can run the remedy, a Fix button; a digest goes out at 09:00.

import { formatDateTime, humanDuration } from "./format.js";

/**
 * @typedef {"action_done" | "action_failed" | "action_deferred" |
 *   "schedule_missed" | "incident" | "host_event" | "fleet_check" |
 *   "alert" | "alert_resolved"} NoticeKind
 * @typedef {"critical" | "warning" | "info" | "ok"} NoticeLevel
 * @typedef {{state: "sent"} | {state: "failed", why: string} |
 *   {state: "skipped", why: string} | {state: "by_sender", who: string}} Push
 * @typedef {{action: string, stack: string, args?: Record<string, string>,
 *   label: string}} Fix a remedy the dashboard runs: the action dialog it
 *   opens, prefilled (Kenny, 2026-09-30)
 * @typedef {{id: number, at: number, kind: NoticeKind, stack?: string,
 *   title: string, body: string, job?: number, read: boolean,
 *   push: Push, level?: NoticeLevel, since?: number, consequence?: string,
 *   remedy?: string, link?: string, source?: string, key?: string,
 *   fixes?: Fix[]}} Notice
 * @typedef {{push: boolean, muted_stacks: string[],
 *   snooze_until?: number | null, digest_at?: string | null}} NotifySettings
 * @typedef {{day: string, at: number, count: number, push: Push}} DigestRecord
 * @typedef {{notices: Notice[], unread: number,
 *   unread_by_stack: Record<string, number>, settings: NotifySettings,
 *   snoozed: boolean, last_digest?: DigestRecord | null}} NotifySnapshot
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
  host_event: { label: "host", tone: "info" },
  fleet_check: { label: "nightly check", tone: "warn" },
  alert: { label: "alert", tone: "bad" },
  alert_resolved: { label: "alert resolved", tone: "ok" },
};

/** @param {NoticeKind} k */
export const kindBadge = (k) => KINDS[k] ?? { label: String(k), tone: "info" };

/** The kind column's sort order. */
export const KIND_ORDER =
  "alert,incident,action failed,nightly check,schedule missed,host,action deferred,alert resolved,action done";

/** @type {Record<NoticeLevel, {label: string, tone: "ok" | "warn" | "bad" | "info"}>} */
const LEVELS = {
  critical: { label: "urgent", tone: "bad" },
  warning: { label: "warning", tone: "warn" },
  info: { label: "info", tone: "info" },
  ok: { label: "ok", tone: "ok" },
};

/**
 * How bad a notice is (decision notify-detail); a notice from before
 * levels existed reads as its kind says.
 * @param {Notice} n
 */
export function levelBadge(n) {
  const l = n.level ?? "info";
  if (l === "info" && ["action_failed", "incident"].includes(n.kind))
    return LEVELS.warning;
  return LEVELS[l] ?? LEVELS.info;
}

/** The level column's sort order, worst first. */
export const LEVEL_ORDER = "urgent,warning,info,ok";

/**
 * A Fix button's words.
 * @param {Fix} f
 */
export const fixLabel = (f) => `Fix: ${f.label}`;

/**
 * What became of a notice's push, in words.
 * @param {Push} p
 */
export function pushText(p) {
  if (!p) return "—";
  if (p.state === "sent") return "sent";
  if (p.state === "failed") return `failed: ${p.why}`;
  if (p.state === "by_sender") return `pushed by ${p.who}`;
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
    text: `Snoozed until ${formatDateTime(until, opts)} (${humanDuration(until - now)} from now)`,
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
    level: levelBadge(n),
    stack: n.stack ?? "—",
    title: n.title,
    body: n.body,
    since: n.since ?? null,
    consequence: n.consequence ?? "",
    remedy: n.remedy ?? "",
    link: n.link ?? null,
    fixes: n.fixes ?? [],
    push: pushText(n.push),
    read: n.read ? "read" : "unread",
    job: n.job ?? null,
  }));
}

/**
 * The last digest in words (decision daily-digest).
 * @param {DigestRecord | null | undefined} d
 * @param {TimeOptions} [opts]
 */
export function digestText(d, opts) {
  if (!d) return "No digest sent yet.";
  const when = formatDateTime(d.at, opts);
  if (d.push.state === "sent")
    return `Last digest ${when}: ${d.count} thing(s) waited, pushed.`;
  return `Last digest ${when}: ${d.count} thing(s) waited, ${pushText(d.push)}.`;
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
  const k = n.level ? levelBadge(n) : kindBadge(n.kind);
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
    digest_at: out.digest_at ?? null,
  };
  if (out.snooze_until != null) body.snooze_until = out.snooze_until;
  return body;
}
