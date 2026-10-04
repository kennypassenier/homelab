// redesign-final-c3 (3.71.0's final review, 2026-10-04): System ›
// Notification rules as the approved notifications.html draws its rules —
// a Delivery card (push, the daily digest, a snooze segment) and a Per
// stack card — without the notice list, which lives in the Inbox. The
// words and the snooze choices, pure, so `node --test` holds them; the
// page (pages/notifications.js) draws them.

/**
 * @typedef {{push: boolean, digest_at?: string | null,
 *   snooze_until?: number | null, muted_stacks?: string[]}} Rules
 */

import { formatDateTime } from "./format.js";

/**
 * A moment as the demos write it: "Sat 4 Oct, 14:00" (never dd/mm/yyyy).
 * @param {number} unix seconds
 * @param {string} [timeZone] the viewer's by default; tests pin it
 */
export function dayTime(unix, timeZone) {
  return formatDateTime(unix, { timeZone });
}

/**
 * The snooze segment's choices (the demo's 1 h · 4 h · Until 07:00 ·
 * 1 day), each in minutes from `now`; "Until 07:00" is the next 07:00 on
 * the viewer's clock.
 * @param {number} now unix seconds
 * @returns {{value: string, label: string, minutes: number, hint: string}[]}
 */
export function snoozeChoices(now) {
  const d = new Date(now * 1000);
  const seven = new Date(d);
  seven.setHours(7, 0, 0, 0);
  if (seven.getTime() <= d.getTime()) seven.setDate(seven.getDate() + 1);
  const morning = Math.max(
    1,
    Math.round((seven.getTime() - d.getTime()) / 60000),
  );
  return [
    {
      value: "60",
      label: "1 h",
      minutes: 60,
      hint: "Hold every push for an hour; the list keeps filling",
    },
    {
      value: "240",
      label: "4 h",
      minutes: 240,
      hint: "Hold every push for four hours",
    },
    {
      value: "morning",
      label: "Until 07:00",
      minutes: morning,
      hint: "Hold every push until 07:00",
    },
    {
      value: "1440",
      label: "1 day",
      minutes: 1440,
      hint: "Hold every push for a day",
    },
  ];
}

/**
 * The header's push chip: "Push on · not snoozed", "Snoozed until Sat 4
 * Oct, 14:00", "Push off".
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
 * The digest's line under its setting.
 * @param {Rules | null | undefined} s
 * @param {{at: number, count: number} | null | undefined} last the last digest
 * @param {string} [timeZone]
 */
export function digestWords(s, last, timeZone) {
  if (!s?.digest_at) return "No digest: only urgent pushes.";
  const then = last
    ? ` Last one ${dayTime(last.at, timeZone)}: ${last.count} ${last.count === 1 ? "thing" : "things"} waited.`
    : " None sent yet.";
  return `One push at ${s.digest_at} with what still waits, worst first.${then}`;
}
