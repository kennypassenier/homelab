// feat-shell-4 (redesign 3.71.0, FLOWS.md §1, decision "Inbox"): the one
// list of everything waiting for a person, worst first, and its counter.
// Health, the bell, the host's questions and the stale images all became
// the Inbox (Kenny approved 2026-10-03); the bar's counter, the tab title,
// the palette's Inbox section and the Inbox page all read THIS list, so the
// number on the badge is always the number of rows the page shows — and
// always the exact number (never "9+", Kenny 2026-10-03).
//
// Pure half: `inboxItems` (sources in, rows out) and `countText`. Live
// half: `inboxNow()` and `onInbox()` over the store (the host's questions),
// act (the notices) and every extra source a page adds with
// `setInboxSource` (the Inbox page's slower reads: stale images, today).

import { askView, openAsks } from "./asks.js";
import { settledNotices } from "./inboxresolve.js";
import { levelBadge } from "./notices.js";
import { contentKey } from "./quiet.js";

/**
 * @typedef {"bad" | "warn" | "info"} Severity
 * @typedef {{key: string, severity: Severity, title: string, why: string,
 *   href: string, stack: string | null, at: number, source: string,
 *   count?: number, first_at?: number, ids?: number[], job?: number,
 *   problem?: {what: string, why?: string, remedy?: string},
 *   snoozeKey?: string}} InboxItem
 *   `key`: stable over reads; `source`: which list it came from; `count`
 *   and `first_at`: how often the same thing was said and since when, for
 *   a row that stands for several notices; `ids`: the notices a row stands
 *   for, all marked seen together; `snoozeKey`: what a snooze is kept under
 *   when `key` moves (a group of notices takes the newest notice's key);
 *   `job`: the dashboard job a failure notice is about (its newest), for
 *   the row's "Open the job" and "Run it again".
 * @typedef {{key: string, until: number, title: string, href?: string,
 *   remind: boolean, set_at: number}} Snooze one row the server holds
 *   snoozed (ask-kp11be-7)
 */

/** Notice kinds that report something finished well: never an Inbox row. */
const QUIET_KINDS = new Set(["action_done", "alert_resolved", "up"]);

const ORDER = /** @type {Record<Severity, number>} */ ({
  bad: 0,
  warn: 1,
  info: 2,
});

/**
 * The Inbox's rows, worst first, then newest first.
 * @param {{asks?: import("./asks.js").Ask[],
 *   notices?: import("./notices.js").Notice[], now: number,
 *   extra?: InboxItem[]}} src
 * @returns {InboxItem[]}
 */
export function inboxItems(src) {
  /** @type {InboxItem[]} */
  const out = [];
  for (const a of openAsks(src.asks ?? [], src.now)) {
    const v = askView(a, src.now);
    out.push({
      key: `ask:${v.key}`,
      severity: "bad",
      title: v.title,
      why: `${v.what} (${v.left})`,
      href: `/inbox#ask-${encodeURIComponent(v.key)}`,
      stack: null,
      at: a.asked_at,
      source: "asks",
    });
  }
  // fix-inbox-group-1 (Kenny, 2026-10-09: "nu wordt ik gespammed met
  // 'opnsense does not answer' ... Dit kan opgelost worden met een
  // counter"): the same thing said again is one row, newest first, that
  // counts how often and says since when.
  /** @type {Map<string, InboxItem>} */
  const same = new Map();
  // inbox-resolved-clears-1: what a later event already settled is no row.
  const settled = settledNotices(src.notices ?? []);
  for (const n of src.notices ?? []) {
    if (n.read || QUIET_KINDS.has(n.kind) || n.level === "ok") continue;
    if (settled.has(n.id)) continue;
    const tone = levelBadge(n).tone;
    const group = `${n.stack ?? ""}|${n.kind}|${n.level ?? ""}|${n.title}`;
    const had = same.get(group);
    if (had) {
      had.count = (had.count ?? 1) + 1;
      had.ids = [...(had.ids ?? []), n.id];
      had.first_at = Math.min(had.first_at ?? had.at, n.at);
      if (n.at > had.at) {
        had.at = n.at;
        had.key = `notice:${n.id}`;
        had.why = n.consequence || n.body || "";
        if (n.job != null) had.job = n.job;
      }
      continue;
    }
    /** @type {InboxItem} */
    const row = {
      key: `notice:${n.id}`,
      severity: tone === "bad" ? "bad" : tone === "warn" ? "warn" : "info",
      title: n.title,
      why: n.consequence || n.body || "",
      href: n.link || `/inbox#notice-${n.id}`,
      stack: n.stack ?? null,
      at: n.at,
      first_at: n.at,
      count: 1,
      ids: [n.id],
      ...(n.job != null ? { job: n.job } : {}),
      source: "notices",
      snoozeKey: `notice-group:${group}`,
      // kp11: the failure's fold ("What went wrong") reads these.
      ...(n.remedy
        ? {
            problem: {
              what: n.body || n.title,
              ...(n.consequence ? { why: n.consequence } : {}),
              remedy: n.remedy,
            },
          }
        : {}),
    };
    same.set(group, row);
    out.push(row);
  }
  const seen = new Set(out.map((i) => i.key));
  for (const i of src.extra ?? []) if (!seen.has(i.key)) out.push(i);
  return out.sort(
    (a, b) => ORDER[a.severity] - ORDER[b.severity] || b.at - a.at,
  );
}

/**
 * A counter's text: the exact number, never capped ("9+" was the bell's
 * until 3.71.0), and empty for none.
 * @param {number} n
 */
export function countText(n) {
  const v = Math.max(0, Math.floor(Number(n) || 0));
  return v === 0 ? "" : String(v);
}

/**
 * The worst severity among the rows, for the badge's colour; null for none.
 * @param {InboxItem[]} items
 * @returns {Severity | null}
 */
export const worst = (items) => items[0]?.severity ?? null;

// ---- live half -------------------------------------------------------

/** @type {Map<string, InboxItem[]>} */
const extras = new Map();
/** @type {Set<() => void>} */
const listeners = new Set();
/** @type {Snooze[]} */
let snoozes = [];

/**
 * The snoozes the server holds (rowsnooze.js): their rows leave the list and
 * the counter until their moment.
 * @param {Snooze[]} list
 */
export function setSnoozes(list) {
  snoozes = list;
  changed();
}

/**
 * The snoozes still running as of now, soonest first.
 * @param {number} [now] unix seconds
 */
export const snoozedNow = (now = Date.now() / 1000) =>
  snoozes.filter((s) => s.until > now).sort((a, b) => a.until - b.until);

/** @type {() => {asks: import("./asks.js").Ask[], notices: import("./notices.js").Notice[] | null}} */
let read = () => ({ asks: [], notices: null });

/**
 * Where the live half reads the host's questions and the notices from
 * (chrome.js wires the store and act; a test wires its own).
 * @param {typeof read} reader
 */
export function wireInbox(reader) {
  read = reader;
  changed();
}

/** Tell every listener the Inbox may have changed. */
export function changed() {
  for (const f of listeners) f();
}

/**
 * Add or replace one extra source's rows (an Inbox page's slower read);
 * `null` takes the source out again.
 * @param {string} name
 * @param {InboxItem[] | null} items
 */
export function setInboxSource(name, items) {
  const before = extras.get(name);
  if (items == null) {
    if (!extras.delete(name)) return;
  } else {
    const next = items.map((i) => ({ ...i, source: name }));
    // refresh-one-minute-1: a background read that brings the same rows
    // changes nothing on screen.
    if (before && contentKey(before) === contentKey(next)) return;
    extras.set(name, next);
  }
  changed();
}

/**
 * The Inbox's rows as of now, and whether its live sources have answered
 * yet (`ready`: the notices were read once).
 * @returns {{items: InboxItem[], ready: boolean}}
 */
export function inboxNow() {
  const r = read();
  const now = Date.now() / 1000;
  const away = new Set(snoozedNow(now).map((s) => s.key));
  return {
    items: inboxItems({
      asks: r.asks,
      notices: r.notices ?? [],
      now,
      extra: [...extras.values()].flat(),
    }).filter((i) => !away.has(i.snoozeKey ?? i.key)),
    ready: r.notices != null,
  };
}

/**
 * @param {() => void} f called whenever the Inbox may have changed
 * @returns {() => void} stop
 */
export function onInbox(f) {
  listeners.add(f);
  return () => listeners.delete(f);
}
