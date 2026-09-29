// What a data table says while it loads, when it has nothing and when it
// failed (Kenny, 2026-09-29: "Als we componenten gebruiken, dan moeten we
// alle toepasselijke features ook gebruiken"). Pure words, so node tests
// them; dom.js puts them in kp's slots.

import { humanDuration } from "./format.js";

/**
 * The status line's words while a table loads: what it is doing, how long
 * it usually takes and, on a refresh, which rows stay on screen meanwhile.
 * kp's datatable counts the seconds after them itself (busy({ since })).
 * @param {{words: string, expect?: number | null, shownFrom?: string | null}} b
 *   expect: the usual duration in seconds (the host's slow reads);
 *   shownFrom: the time of the rows still shown (a refresh).
 * @returns {string}
 */
export function busyWords(b) {
  const parts = [b.words];
  if (b.expect)
    parts.push(`It usually takes about ${humanDuration(b.expect)}.`);
  if (b.shownFrom) parts.push(`Showing the rows from ${b.shownFrom}.`);
  return parts.join(" ");
}

/**
 * The failed slot's words (kp's fail(reason)): what could not be read,
 * why, and what to do, one per line.
 * @param {{what: string, why: string, fix?: string | null}} e
 * @returns {string}
 */
export function failReason(e) {
  return [
    `Could not read ${e.what}.`,
    `Why: ${e.why}`,
    ...(e.fix ? [`What to do: ${e.fix}`] : []),
  ].join("\n");
}

/**
 * The empty box under a table: "there is nothing" and "nothing matches"
 * read differently and offer different ways out (kp's #states).
 * @param {{total: number, query: string, filters: Record<string, unknown>}} view
 *   the kp datatable's view
 * @param {string} nothing the words for a table with no rows at all
 * @returns {{title: string, body: string, clear: boolean}}
 */
export function emptyWords(view, nothing) {
  if (view.total === 0) return { title: nothing, body: "", clear: false };
  const nFilters = Object.keys(view.filters ?? {}).length;
  const by = [
    view.query ? `the search "${view.query}"` : "",
    nFilters === 1 ? "a filter" : nFilters > 1 ? `${nFilters} filters` : "",
  ].filter(Boolean);
  return {
    title: `No row matches ${by.length ? by.join(" and ") : "the search"}.`,
    body: `The table holds ${view.total} row${view.total === 1 ? "" : "s"}; clear the search and filters to see ${view.total === 1 ? "it" : "them"}.`,
    clear: true,
  };
}
