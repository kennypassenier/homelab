// feat-overview-8: the keyboard shortcuts, as data. The same list draws the
// help sheet ("?") and drives the keys, so the sheet cannot promise a key
// that does nothing. Two-key shortcuts ("g o") wait `CHORD_MS` for the
// second key.

import { NAV, OTHER_PAGES, STACK_TABS, stackHref } from "./router.js";

const PAGES = [...NAV, ...OTHER_PAGES];

export const CHORD_MS = 1500;

/**
 * @typedef {{keys: string, what: string}} Shortcut
 * @typedef {{group: string, shortcuts: Shortcut[]}} ShortcutGroup
 * @typedef {{navigate: string} | {focusSearch: true} | null} KeyAction
 * @typedef {{prefix: string | null, at: number}} ChordState
 */

/** @type {Record<string, string>} second key after "g" → page */
const GO = {
  o: "overview",
  h: "host",
  a: "activity",
  t: "timeline",
  c: "checks",
  d: "doctor",
  j: "jobs",
  s: "schedules",
  n: "notifications",
  f: "firewall",
  e: "settings",
  // TUI parity round.
  y: "today",
  l: "log",
  p: "apply",
  x: "shell",
};

/** @type {ShortcutGroup[]} */
export const SHORTCUTS = [
  {
    group: "Anywhere",
    shortcuts: [
      { keys: "Ctrl K", what: "Open the command palette" },
      { keys: "?", what: "Show these shortcuts" },
      { keys: "/", what: "Search the first table on the page" },
      ...Object.entries(GO).map(([k, page]) => ({
        keys: `g ${k}`,
        what: `Go to ${PAGES.find((n) => n.page === page)?.label}`,
      })),
    ],
  },
  {
    group: "On a stack's page",
    shortcuts: [
      {
        keys: `1 … ${STACK_TABS.length}`,
        what: `Open a tab (${STACK_TABS.map((t) => t.label).join(", ")})`,
      },
      { keys: "[ ]", what: "The previous or next tab" },
    ],
  },
];

/** @returns {ChordState} */
export const idle = () => ({ prefix: null, at: 0 });

/**
 * What one key press does. `route` is the page the key was pressed on.
 * @param {ChordState} state
 * @param {string} key KeyboardEvent.key
 * @param {number} now milliseconds
 * @param {import("./router.js").Route} route
 * @returns {{state: ChordState, action: KeyAction}}
 */
export function keyAction(state, key, now, route) {
  const chord = state.prefix != null && now - state.at <= CHORD_MS;
  if (chord && state.prefix === "g") {
    const page = GO[key.toLowerCase()];
    const n = PAGES.find((x) => x.page === page);
    return { state: idle(), action: n ? { navigate: n.href } : null };
  }
  if (key === "g") return { state: { prefix: "g", at: now }, action: null };
  if (key === "/") return { state: idle(), action: { focusSearch: true } };
  if (route.page === "stack") {
    const i = STACK_TABS.findIndex((t) => t.tab === route.tab);
    let next = -1;
    if (/^[1-9]$/.test(key)) next = Number(key) - 1;
    else if (key === "[") next = i - 1;
    else if (key === "]") next = i + 1;
    if (next >= 0 && next < STACK_TABS.length && next !== i)
      return {
        state: idle(),
        action: { navigate: stackHref(route.name, STACK_TABS[next].tab) },
      };
  }
  return { state: idle(), action: null };
}
