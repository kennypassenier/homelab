// redesign-final-h3 (3.71.0's final review, 2026-10-04): the Restore flow
// as the approved flows/restore.html (FLOWS.md §5, task 5): Which app ·
// Which night · Confirm · Restore and check, one page with its own address
// like the Update flow, where a picker dialog used to open a second
// dialog. Pure, so `node --test` holds the nights, the words and the
// address; pages/restore.js draws them.

import { formatClock, formatDateTime, formatDay } from "./format.js";

/**
 * @typedef {{id: string, short_id: string, time: number,
 *   size_bytes?: number | null}} Snap
 * @typedef {{key: string, label: string, snap: Snap | null,
 *   missed: boolean}} Night
 */

/**
 * The flow's address: `/backups/restore?stack=…[&app=…][&snapshot=…]`.
 * @param {string | null} stack
 * @param {string | null} [app]
 * @param {string | null} [snapshot]
 */
export function restoreHref(stack, app = null, snapshot = null) {
  const p = new URLSearchParams();
  if (stack) p.set("stack", stack);
  if (app) p.set("app", app);
  if (snapshot) p.set("snapshot", snapshot);
  const q = p.toString();
  return `/backups/restore${q ? `?${q}` : ""}`;
}

/** @param {Date} d */
const dayKey = (d) =>
  `${d.getFullYear()}-${String(d.getMonth() + 1).padStart(2, "0")}-${String(d.getDate()).padStart(2, "0")}`;

/**
 * The nights to pick from, newest first: each snapshot of the app (a
 * night with two keeps both, the later first), and a night of the last
 * `days` without one shown as missed — never hidden (the demo's "Mon 28
 * Sep — no snapshot (missed)"). Today without a snapshot yet is no miss.
 * @param {Snap[]} snaps
 * @param {number} now unix seconds
 * @param {number} [days] how many nights back a missed one is shown
 * @returns {Night[]}
 */
export function restoreNights(snaps, now, days = 14) {
  const sorted = [...snaps].sort((a, b) => b.time - a.time);
  const today = dayKey(new Date(now * 1000));
  /** @type {Map<string, Snap[]>} */
  const byDay = new Map();
  for (const s of sorted) {
    const k = dayKey(new Date(s.time * 1000));
    byDay.set(k, [...(byDay.get(k) ?? []), s]);
  }
  const oldest = sorted.length ? sorted[sorted.length - 1].time : now;
  /** @type {Night[]} */
  const out = [];
  const d = new Date(now * 1000);
  d.setHours(12, 0, 0, 0);
  for (let i = 0; i < days; i++) {
    const k = dayKey(d);
    const unix = d.getTime() / 1000;
    if (k < dayKey(new Date(oldest * 1000))) break;
    const here = byDay.get(k) ?? [];
    for (const s of here)
      out.push({
        key: s.short_id,
        label: `${k === today ? "Today" : formatDay(s.time)} ${formatClock(s.time)}`,
        snap: s,
        missed: false,
      });
    if (!here.length && k !== today)
      out.push({
        key: `missed:${k}`,
        label: formatDay(unix),
        snap: null,
        missed: true,
      });
    byDay.delete(k);
    d.setDate(d.getDate() - 1);
  }
  // Older snapshots than the window: still offered, newest first.
  for (const s of sorted)
    if (byDay.has(dayKey(new Date(s.time * 1000))))
      out.push({
        key: s.short_id,
        label: formatDateTime(s.time),
        snap: s,
        missed: false,
      });
  return out;
}

/**
 * Step 3's list, the demo's words: what stops, the safety copy (the undo),
 * what comes back, the check.
 * @param {{stack: string, app: string, night: string, native: boolean}} x
 * @returns {string[]}
 */
export function whatWillHappen(x) {
  return [
    `${x.app} stops (about 1 min); ${x.native ? "nothing else on" : "the rest of"} ${x.stack} keeps running.`,
    `Today's data of ${x.app} is copied aside as a safety copy (kept 7 days: this is your undo).`,
    `The data from ${x.night} is put back.`,
    `${x.app} starts and is checked: healthcheck, version, logs.`,
  ];
}

/**
 * The step the flow stands on, from what is chosen: 1 Which app, 2 Which
 * night, 3 Confirm, 4 Restore and check.
 * @param {{app: string | null, night: string | null, running: boolean}} s
 */
export const restoreStep = (s) =>
  s.running ? 4 : !s.app ? 1 : !s.night ? 2 : 3;
