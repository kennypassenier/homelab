// feat-shell-2 (redesign 3.71.0, FLOWS.md §1.2): the command palette finds
// pages AND actions by intent. Word order is free and words match by
// prefix, so "update jellyfin", "jellyfin update", "gateway logs" and
// "logs gateway" all find what they mean; before 3.71.0 the palette matched
// one contiguous substring and answered "No commands" to all four
// (measured: flows/current/t17-palette-update-gateway-1894.png).
//
// Pure: commands in, the ranked list out. chrome.js draws it into kp's
// palette and keeps kp's own keyboard and listbox behaviour.

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
export const MAX_SHOWN = 40;

/**
 * The searchable words of a text: lower-case, split on anything that is not
 * a letter, digit, dot or hyphen, each hyphenated word also split into its
 * parts ("kp-soft" is found by "kp", "kp-s" and "soft").
 * @param {string} text
 * @returns {string[]}
 */
export function words(text) {
  const out = [];
  for (const w of text.toLowerCase().split(/[^\p{L}\p{N}.-]+/u)) {
    if (!w) continue;
    out.push(w);
    if (w.includes("-")) out.push(...w.split("-").filter(Boolean));
  }
  return out;
}

/**
 * The words a person typed.
 * @param {string} query
 */
export const queryWords = (query) =>
  query
    .toLowerCase()
    .split(/\s+/)
    .map((w) => w.replace(/^[^\p{L}\p{N}]+|[^\p{L}\p{N}]+$/gu, ""))
    .filter(Boolean);

/**
 * Whether every typed word starts one of the text's words, in any order.
 * An empty query matches everything.
 * @param {string} text
 * @param {string} query
 */
export function matchIntent(text, query) {
  const want = queryWords(query);
  if (want.length === 0) return true;
  const have = words(text);
  return want.every((w) => have.some((h) => h.startsWith(w)));
}

/**
 * @typedef {{id: string, group: string, label: string, href?: string,
 *   run?: () => void, hint?: string, keys?: string, words?: string,
 *   stack?: string, featured?: boolean}} Command
 *   `words`: what else a person may type for it (never shown);
 *   `stack`: the stack it is about, so the open stack's come first;
 *   `featured`: shown before anything is typed.
 */

/**
 * How well a command answers the query; -1 when it does not. A word found
 * in the label counts double one found in the hint or the hidden words;
 * the label's first word (the verb) answering the first typed word, and an
 * exact word rather than a prefix, each add a little.
 * @param {Command} c
 * @param {string[]} want
 */
export function score(c, want) {
  if (want.length === 0) return 0;
  const label = words(c.label);
  const rest = words(`${c.hint ?? ""} ${c.words ?? ""}`);
  let s = 0;
  for (const [i, w] of want.entries()) {
    const li = label.findIndex((h) => h.startsWith(w));
    if (li >= 0) {
      s += 4;
      if (label[li] === w) s += 1;
      if (i === 0 && li === 0) s += 2;
      continue;
    }
    if (rest.some((h) => h.startsWith(w))) {
      s += 1;
      continue;
    }
    return -1;
  }
  return s;
}

/**
 * The palette's rows for a query: only the commands that answer it, grouped
 * Inbox → Do → Go to → Theme, the best answer first in each group (the
 * open stack's own actions before every other stack's), at most
 * `MAX_SHOWN`. With nothing typed, it shows the Inbox, the actions on the
 * open stack plus the fleet-wide ones, and the places to go — never all
 * of every stack's actions at once.
 * @param {Command[]} commands
 * @param {string} query
 * @param {{here?: string | null}} [ctx] the stack whose hub is open
 * @returns {{group: string, commands: Command[]}[]}
 */
export function rankCommands(commands, query, ctx = {}) {
  const want = queryWords(query);
  const here = ctx.here ?? null;
  const rank = (/** @type {string} */ g) => {
    const i = SECTIONS.indexOf(/** @type {any} */ (g));
    return i < 0 ? SECTIONS.length : i;
  };
  const scored = commands
    .map((c, i) => ({ c, i, s: score(c, want) }))
    .filter(({ c, s }) => {
      if (s < 0) return false;
      if (want.length > 0) return true;
      // Nothing typed: a short, useful start — the Inbox, what is featured
      // (New stack, Deploy all changes, each stack's hub), the open stack's
      // own commands and the places that belong to no stack.
      if (c.group === "Inbox" || c.featured) return true;
      if (c.stack != null) return c.stack === here;
      return c.group === "Go to";
    })
    .map((x) => ({
      ...x,
      s: x.s + (here != null && x.c.stack === here ? 3 : 0),
    }))
    .sort((a, b) => rank(a.c.group) - rank(b.c.group) || b.s - a.s || a.i - b.i)
    .slice(0, MAX_SHOWN);
  /** @type {Map<string, Command[]>} */
  const groups = new Map();
  for (const { c } of scored) {
    const g = groups.get(c.group);
    if (g) g.push(c);
    else groups.set(c.group, [c]);
  }
  return [...groups].map(([group, list]) => ({ group, commands: list }));
}

/**
 * What the palette says when nothing answers: when the first word is a verb
 * it knows, which stacks that verb works on ("There is no stack called
 * jellyfin. Update works on: admin, gateway…"); otherwise what to try.
 * @param {string} query
 * @param {{verbs: string[], stacks: string[]}} known
 * @returns {string}
 */
export function noMatchText(query, known) {
  const want = queryWords(query);
  const verb = known.verbs.find(
    (v) => want.length > 1 && words(v)[0]?.startsWith(want[0]),
  );
  if (verb && known.stacks.length > 0)
    return `There is no stack called "${want.slice(1).join(" ")}". ${verb} works on: ${known.stacks.join(", ")}.`;
  return "Nothing matches. Try a stack name, a page (Backups, Host) or a verb (update, restore, logs).";
}
