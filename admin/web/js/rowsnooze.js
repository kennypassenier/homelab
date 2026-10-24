// ask-kp11be-7: the Inbox's per-row snooze and remind-me, the browser half.
// The server holds the snoozes (`/data/inbox-snoozes`, capped, one file);
// this module reads them on every page (so the bar's counter drops a
// snoozed row everywhere), keeps them in step with the live event, and
// sends a snooze or its end.

import { send } from "./act.js";
import { fetchJson } from "./dom.js";
import { changed, setSnoozes, snoozeKeyOf } from "./inboxlist.js";
import { listen } from "./store.js";

/** How far ahead the picker reaches (the server takes a year). */
export const SNOOZE_MAX = "90d";

/** @typedef {import("./inboxlist.js").Snooze} Snooze */
/** @typedef {"loading" | "ready" | "error"} SnoozeState */

/** @type {SnoozeState} */
let state = "loading";
/** @type {string} */
let why = "";
/** @type {Set<() => void>} */
const listeners = new Set();
let started = false;

/** Whether the snoozes have been read, and why not when they could not. */
export const snoozeStatus = () => ({ state, why });

/**
 * @param {() => void} f called when the read state changes
 * @returns {() => void} stop
 */
export function onSnoozeStatus(f) {
  listeners.add(f);
  return () => listeners.delete(f);
}

/** @param {SnoozeState} next @param {string} [reason] */
function setState(next, reason = "") {
  if (next === state && reason === why) return;
  state = next;
  why = reason;
  for (const f of listeners) f();
}

/** @param {any} body */
function take(body) {
  setSnoozes(Array.isArray(body?.snoozes) ? body.snoozes : []);
  setState("ready");
}

/** Read the snoozes again. */
export async function readSnoozes() {
  const r = await fetchJson("/data/inbox-snoozes", "the snoozed rows");
  if (r.ok) take(r.body);
  else setState("error", `${r.error.what}: ${r.error.why}`);
}

/**
 * Snooze one Inbox row until a moment, optionally leaving a reminder
 * notice when it comes back.
 * @param {{key: string, snoozeKey?: string, title: string, href?: string}} row
 * @param {number} untilS unix seconds
 * @param {boolean} remind
 */
export async function snoozeRow(row, untilS, remind) {
  const r = await send(
    "POST",
    "/data/inbox-snoozes",
    {
      key: snoozeKeyOf(row),
      until: untilS,
      title: row.title,
      ...(row.href?.startsWith("/") ? { href: row.href } : {}),
      remind,
    },
    "the snooze",
  );
  if (r.ok) take(r.body);
  return r;
}

/** End a snooze by the key it was made under. @param {string} key */
export async function endSnooze(key) {
  const r = await send(
    "POST",
    "/data/inbox-snoozes",
    { key, until: null },
    "ending the snooze",
  );
  if (r.ok) take(r.body);
  return r;
}

/** Start reading and following the snoozes (chrome.js, once). */
export function startRowSnoozes() {
  if (started) return;
  started = true;
  listen("inbox_snoozes", (body) => take(body));
  void readSnoozes().catch(() => {});
  // A snooze ends by itself: the list is drawn again when one is due.
  setInterval(() => changed(), 30_000);
  listen("resync", () => void readSnoozes().catch(() => {}));
}
