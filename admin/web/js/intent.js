// feat-shell-2 (redesign 3.71.0, FLOWS.md §1.2): the command palette finds
// pages AND actions by intent. The words-in-any-order matching, the scoring
// and the "no match" sentence are kp-themes' own intent.js (extracted from
// this file in kp-themes 11.0.0, step-kp11-38); what stays here is only
// what is this dashboard's: the section order and the short start.
import {
  INTENT_MAX_SHOWN,
  noMatchText,
  rankCommands,
} from "/static/kp/js/intent.js";

/**
 * The palette's sections, in the order they are shown (FLOWS.md §1.2):
 * open Inbox items, then actions, then places, then themes.
 */
export const SECTIONS = /** @type {const} */ ([
  "Inbox",
  "Do",
  "Go to",
  "Theme",
]);

/** The most rows the palette shows at once; the rest wait for more words. */
export const MAX_SHOWN = INTENT_MAX_SHOWN;

/**
 * @typedef {{id: string, group: string, label: string, href?: string,
 *   run?: () => void, hint?: string, keys?: string, words?: string,
 *   stack?: string, featured?: boolean}} Command
 *   `words`: what else a person may type for it (never shown);
 *   `stack`: the stack it is about, so the open stack's come first;
 *   `featured`: shown before anything is typed.
 */

/**
 * The palette's rows for a query: only the commands that answer it, grouped
 * Inbox → Do → Go to → Theme, the best answer first in each group (the
 * open stack's own actions before every other stack's). With nothing typed
 * it shows the Inbox, what is featured, the open stack's own commands and
 * the places that belong to no stack: never all of every stack's actions.
 * @param {Command[]} commands
 * @param {string} query
 * @param {{here?: string | null}} [ctx] the stack whose hub is open
 * @returns {{group: string, commands: Command[]}[]}
 */
export function rankPalette(commands, query, ctx = {}) {
  const start = commands.map((c) => ({
    ...c,
    context: c.stack,
    featured:
      c.featured === true ||
      c.group === "Inbox" ||
      (c.group === "Go to" && c.stack == null),
  }));
  return rankCommands(start, query, {
    context: ctx.here ?? null,
    groupOrder: SECTIONS,
  });
}

/**
 * What the palette says when nothing answers: when the first word is a verb
 * it knows, which stacks that verb works on; otherwise what to try.
 * @param {string} query
 * @param {{verbs: string[], stacks: string[]}} known
 * @returns {string}
 */
export const noMatch = (query, known) =>
  noMatchText(query, {
    verbs: known.verbs,
    subjects: known.stacks,
    noun: "stack",
    hint: "Nothing matches. Try a stack name, a page (Backups, Host) or a verb (update, restore, logs).",
  });
