// redesign-flows-2 (redesign 3.71.0, FLOWS.md §1 "Needs you", invariant
// 60): the Inbox's slower sources, fed into inboxlist.js on EVERY page — so the
// counter in the bar is the number of rows the Inbox shows wherever you
// are, not only after the Inbox was opened once.
//
//   updates   the fleet check's stale images, its kept last answer
//             (`?last=1`: starts no ~90 s run on a page load)
//   today     Today's broken / attention items, its kept last answer
//   (manual checks are not a source: manual-checks-as-info-1, they are
//   information on the stack's page)
//   address   the house's public address against the one Cloudflare lets in
//             without a login (the server's last reading, eten-home-ip-1)
//   setup     the stacks whose secrets have no copy in the host's vault,
//             live from the fleet snapshot
//
// The Inbox page's "Check again" runs the slow reads for real and hands the
// fresh answers to `feedUpdates` / `feedToday`. Pure rows: inboxrows.js.

import { fetchJson } from "./dom.js";
import { setInboxSource } from "./inboxlist.js";
import {
  homeAddressRows,
  setupRows,
  todayRows,
  updateRows,
} from "./inboxrows.js";
import { REFRESH_MS } from "./quiet.js";
import { startRowSnoozes } from "./rowsnooze.js";
import { current, subscribe } from "./store.js";

/** How often the kept answers are read again. */
const EVERY_MS = REFRESH_MS;

/** @type {any} the last Today answer, re-filtered when Setup moves */
let lastToday = null;
let setupKey = "";
let started = false;

/** The Setup row's stacks, from the fleet snapshot. */
const setupStacks = () =>
  setupRows(current().fleet)[0]?.stacks ?? /** @type {string[]} */ ([]);

/**
 * A stale-images answer (kept or fresh) as the Updates row.
 * @param {any} body
 */
export function feedUpdates(body) {
  setInboxSource("updates", updateRows(body));
}

/**
 * A Today answer (kept or fresh) as its rows.
 * @param {any} body
 */
export function feedToday(body) {
  lastToday = body;
  setInboxSource("today", todayRows(body, setupStacks()));
}

/** The kept answers of the two slow reads; nothing kept leaves them be. */
async function readKept() {
  const [s, t, a] = await Promise.all([
    fetchJson("/data/stale-images?last=1", "the stale images"),
    fetchJson("/data/today?last=1", "today's reading"),
    fetchJson("/data/home-address?last=1", "the home address"),
  ]);
  if (s.ok) feedUpdates(s.body);
  if (t.ok) feedToday(t.body);
  if (a.ok) setInboxSource("address", homeAddressRows(a.body));
}

const paintSetup = () => {
  const rows = setupRows(current().fleet);
  const key = rows[0]?.stacks?.join(",") ?? "";
  if (key === setupKey) return;
  setupKey = key;
  setInboxSource("setup", rows);
  if (lastToday) feedToday(lastToday);
};

/**
 * Start every source once (chrome.js, next to `wireInbox`).
 * @returns {void}
 */
export function startInboxSources() {
  if (started) return;
  started = true;
  subscribe(paintSetup);
  paintSetup();
  void readKept().catch(() => {});
  startRowSnoozes();
  setInterval(() => {
    void readKept().catch(() => {});
  }, EVERY_MS);
}
