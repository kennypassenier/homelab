// redesign-final-c3 (3.71.0's final review, 2026-10-04), rebuilt on
// kp-themes 11.0.0 (feat-kp11demo-4, the review demo app-homelab-inbox,
// approved 2026-10-19): System › Notification rules. The words of the
// rules, pure, so `node --test` holds them; the page (pages/notifications.js)
// draws them. The snooze choices and their moments are kp-themes' own
// snooze picker's now (js/snooze.js), so this file holds only what the
// picker cannot know: how long the server lets a snooze last, the header's
// state word, the last digest in words, and what a rule set would have done
// with a notice.

/**
 * @typedef {{push: boolean, digest_at?: string | null,
 *   snooze_until?: number | null, muted_stacks?: string[]}} Rules
 */

import { formatDateTime } from "./format.js";

/** The longest snooze the server takes, in minutes (a week). */
export const SNOOZE_MAX_MINUTES = 10080;

/**
 * A moment as every page writes it: "04/10/2026 14:00".
 * @param {number} unix seconds
 * @param {string} [timeZone] the viewer's by default; tests pin it
 */
export function dayTime(unix, timeZone) {
  return formatDateTime(unix, { timeZone });
}

/**
 * The minutes a snooze until `untilMs` is, as the server takes them: at
 * least one, at most a week.
 * @param {number} untilMs
 * @param {number} nowMs
 */
export const snoozeMinutes = (untilMs, nowMs) =>
  Math.min(
    SNOOZE_MAX_MINUTES,
    Math.max(1, Math.ceil((untilMs - nowMs) / 60000)),
  );

/**
 * The card's push state word: "Push on · not snoozed", "Snoozed until
 * 04/10/2026 14:00", "Push off".
 * @param {Rules | null | undefined} s
 * @param {number} now
 * @param {string} [timeZone]
 * @returns {{tone: "ok" | "warn" | "", text: string, snoozed: boolean}}
 */
export function pushChip(s, now, timeZone) {
  if (!s) return { tone: "", text: "Reading the rules…", snoozed: false };
  if (!s.push) return { tone: "", text: "Push off", snoozed: false };
  const until = s.snooze_until ?? null;
  if (until != null && until > now)
    return {
      tone: "warn",
      text: `Snoozed until ${dayTime(until, timeZone)}`,
      snoozed: true,
    };
  return { tone: "ok", text: "Push on · not snoozed", snoozed: false };
}

/**
 * The last digest in words, for under the digest's time.
 * @param {{at: number, count: number} | null | undefined} last the last digest
 * @param {string} [timeZone]
 */
export function digestLast(last, timeZone) {
  return last
    ? `Last one ${dayTime(last.at, timeZone)}: ${last.count} ${last.count === 1 ? "thing" : "things"} waited.`
    : "None sent yet.";
}

/**
 * What the rules would do with one notice, in the server's own order
 * (core/notify.rs `decide`): a snooze silences everything, then only an
 * urgent notice pushes, then the global switch, then the stack's.
 * @param {{level?: string, stack?: string}} n
 * @param {Rules} s
 * @param {number} now unix seconds
 * @param {string} [timeZone]
 * @returns {{pushed: boolean, why: string}}
 */
export function pushVerdict(n, s, now, timeZone) {
  if (s.snooze_until != null && s.snooze_until > now)
    return {
      pushed: false,
      why: `Not pushed: every push is snoozed until ${dayTime(s.snooze_until, timeZone)}.`,
    };
  if (n.level !== "critical")
    return {
      pushed: false,
      why: s.digest_at
        ? `Held for the ${s.digest_at} digest: only urgent notices push at once.`
        : "Not pushed: only urgent notices push at once, and there is no daily digest.",
    };
  if (!s.push)
    return { pushed: false, why: "Not pushed: push to the phone is off." };
  if (n.stack && (s.muted_stacks ?? []).includes(n.stack))
    return { pushed: false, why: `Not pushed: ${n.stack} is muted.` };
  return {
    pushed: true,
    why: "Pushed: an urgent notice goes to the phone at once.",
  };
}
