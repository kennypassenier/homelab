// redesign-flows-2 (redesign 3.71.0, FLOWS.md §1 "Needs you", invariant
// 60): the Inbox's slower sources, fed into inboxlist.js on EVERY page — so the
// counter in the bar is the number of rows the Inbox shows wherever you
// are, not only after the Inbox was opened once.
//
//   updates   the fleet check's stale images, its kept last answer
//             (`?last=1`: starts no ~90 s run on a page load)
//   today     Today's broken / attention items, its kept last answer
//   checks    the manual checks a person answers (a cheap host read),
//             again whenever an answer's job finished
//   setup     the stacks whose secrets have no copy in the host's vault,
//             live from the fleet snapshot
//
// The Inbox page's "Check again" runs the slow reads for real and hands the
// fresh answers to `feedUpdates` / `feedToday`. Pure rows: inboxrows.js.

import { onAnswered } from "./answer.js";
import { fetchJson, fetchReport } from "./dom.js";
import { setInboxSource } from "./inboxlist.js";
import { checkRowsOf, setupRows, todayRows, updateRows } from "./inboxrows.js";
import { current, subscribe } from "./store.js";

/** How often the kept answers and the manual checks are read again. */
const EVERY_MS = 10 * 60 * 1000;

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

/** Read the manual checks again. */
export async function readChecks() {
  const r = await fetchReport("/data/manual-checks", "the manual checks");
  if (r.ok) setInboxSource("checks", checkRowsOf(r.report));
}

/** The kept answers of the two slow reads; nothing kept leaves them be. */
async function readKept() {
  const [s, t] = await Promise.all([
    fetchJson("/data/stale-images?last=1", "the stale images"),
    fetchJson("/data/today?last=1", "today's reading"),
  ]);
  if (s.ok) feedUpdates(s.body);
  if (t.ok) feedToday(t.body);
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
  void readChecks().catch(() => {});
  onAnswered(() => void readChecks().catch(() => {}));
  setInterval(() => {
    void readKept().catch(() => {});
    void readChecks().catch(() => {});
  }, EVERY_MS);
}
