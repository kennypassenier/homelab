// feat-overview-8, feat-kpadopt-2 (feat-kp11demo-14): the dashboard's keys,
// as data for kp-themes' shortcut engine (js/shortcuts.js). The engine
// handles the chords ("g s" waits for its second key, the small cap says
// so, "No shortcut on g x" says when none is), warns about a key that needs
// AltGr on an AZERTY keyboard, dims a key that does nothing on this page
// with its reason, and fills the help sheet from the same list, so the sheet
// cannot promise a key that does nothing. What stays here is only what is
// this dashboard's: where each letter goes, and the stack hub's tab keys.

import { STACK_TABS, stackHref } from "./router.js";

/**
 * Where "g" + a letter goes, and what the help sheet calls it. A fixed,
 * local table (never the page registry, pages.js): the sheet must work
 * before `GET /api/kit/pages` has answered. feat-shell-1 (3.71.0): the six
 * areas first, then the pages inside them, at their current addresses.
 * @type {Record<string, {href: string, label: string}>}
 */
const PAGES = {
  home: { href: "/apps", label: "Apps" },
  "needs-you": { href: "/inbox", label: "the Inbox" },
  stacks: { href: "/stacks", label: "Stacks" },
  activity: { href: "/activity", label: "Activity" },
  backups: { href: "/backups", label: "Backups" },
  system: { href: "/system", label: "System" },
  running: { href: "/activity?view=running", label: "Running now" },
  planned: { href: "/activity?view=planned", label: "Planned" },
  hostlog: { href: "/activity?view=host-log", label: "the Host log" },
  host: { href: "/host", label: "Host" },
  metrics: { href: "/charts", label: "Metrics" },
  map: { href: "/map", label: "the Map" },
  firewall: { href: "/firewall", label: "Firewall" },
  settings: { href: "/settings", label: "Host settings" },
  console: { href: "/console", label: "the Console" },
  deployall: { href: "/stacks?deploy-all=1", label: "Deploy all changes" },
};

/** @type {Record<string, string>} second key after "g" → page */
export const GO = {
  u: "home",
  i: "needs-you",
  s: "stacks",
  a: "activity",
  b: "backups",
  y: "system",
  j: "running",
  p: "planned",
  l: "hostlog",
  h: "host",
  m: "metrics",
  v: "map",
  f: "firewall",
  e: "settings",
  x: "console",
  d: "deployall",
};

/** The sheet's group names; the engine lists the scope's own group first. */
export const GROUP_ANYWHERE = "Anywhere";
export const GROUP_STACK = "On a stack's page";
export const GROUP_CONSOLE = "On the Console page";
export const GROUP_SECRETS = "On the Secrets page";
export const GROUP_BACKUPS = "On the Backups page";
export const GROUP_HOST = "On the Host page";
export const GROUP_FIREWALL = "On the Firewall page";
export const GROUP_SCHEDULES = "On the Schedules page";
export const GROUP_MAP = "On the Map page";
export const GROUP_ACTIVITY = "On the Activity page";

/**
 * The address "g" + a letter goes to.
 * @param {string} letter
 * @returns {string | null}
 */
export function goTarget(letter) {
  const page = GO[letter.toLowerCase()];
  return page ? PAGES[page].href : null;
}

/**
 * Where a stack-hub tab key goes: tab number `n` (1-based) or, with `step`,
 * the tab before (-1) or after (+1) the open one. Null when there is no such
 * tab, it is the open one, or the page is not a stack's.
 * @param {import("./router.js").Route} route
 * @param {{n: number} | {step: -1 | 1}} how
 * @returns {string | null}
 */
export function tabTarget(route, how) {
  if (route.page !== "stack") return null;
  const here = STACK_TABS.findIndex((t) => t.tab === route.tab);
  const to = "n" in how ? how.n - 1 : here + how.step;
  if (to < 0 || to >= STACK_TABS.length || to === here) return null;
  return stackHref(route.name, STACK_TABS[to].tab);
}

/**
 * The digit of a physical `Alt+digit` press, or null. Alt + a digit is named
 * by the physical key (`Digit3`): on a Belgian AZERTY keyboard the unshifted
 * digit row types symbols, so the engine (which reads `event.key`) never
 * sees "3" there. No other modified press is ours (fix-kp11-7: no shortcut
 * may need AltGr, which is Ctrl+Alt).
 * @param {{key: string, code: string, altKey: boolean, ctrlKey: boolean,
 *   metaKey: boolean, shiftKey: boolean}} e
 * @returns {number | null}
 */
export function altDigit(e) {
  if (!e.altKey || e.ctrlKey || e.metaKey || e.shiftKey) return null;
  const m = /^(?:Digit|Numpad)([1-9])$/.exec(e.code);
  return m ? Number(m[1]) : null;
}

/**
 * Whether no dialog is open. A page's key does nothing over a dialog: the
 * dialog's own keys (Esc closes it) come first.
 * @returns {boolean}
 */
export const noDialog = () => document.querySelector("dialog[open]") === null;

/**
 * Make a page key stand back while a dialog is open: the engine leaves the
 * key alone (`false`) and the dialog keeps it.
 * @template {unknown} T
 * @param {(event: KeyboardEvent) => T} run
 * @returns {(event: KeyboardEvent) => T | false}
 */
export const unlessDialog = (run) => (event) =>
  noDialog() ? run(event) : false;

/**
 * A key that belongs to one page: the page says what it does while it is
 * mounted (`onPageKey`), and the engine shows the key dimmed, with its
 * reason, on every other page.
 * @type {Map<string, () => void>}
 */
const PAGE_KEYS = new Map();

/**
 * Take a page's key while that page is mounted.
 * @param {"prompt" | "reveal"} name
 * @param {() => void} run
 * @returns {() => void} give it back
 */
export function onPageKey(name, run) {
  PAGE_KEYS.set(name, run);
  return () => {
    if (PAGE_KEYS.get(name) === run) PAGE_KEYS.delete(name);
  };
}

/**
 * Every key of the app, as kp's engine takes them.
 * @param {{navigate: (href: string) => void,
 *   route: () => import("./router.js").Route, focusSearch: () => void}} ctx
 * @returns {import("/static/kp/dist/kp-themes.min.js").Shortcut[]}
 */
export function appShortcuts(ctx) {
  const onStack = () => ctx.route().page === "stack";
  const reason = "Only on a stack's page";
  return [
    {
      keys: "/",
      what: "Search the first table on the page",
      group: GROUP_ANYWHERE,
      run: () => ctx.focusSearch(),
    },
    ...Object.entries(GO).map(([k, page]) => ({
      keys: `g ${k}`,
      what: `Go to ${PAGES[page].label}`,
      group: GROUP_ANYWHERE,
      run: () => ctx.navigate(PAGES[page].href),
    })),
    ...STACK_TABS.map((t, i) => ({
      keys: `Alt ${i + 1}`,
      what: `Open the ${t.label} tab`,
      group: GROUP_STACK,
      when: onStack,
      reason,
      run: () => {
        const to = tabTarget(ctx.route(), { n: i + 1 });
        if (to) ctx.navigate(to);
      },
    })),
    {
      keys: "t p",
      what: "The previous tab",
      group: GROUP_STACK,
      when: onStack,
      reason,
      run: () => {
        const to = tabTarget(ctx.route(), { step: -1 });
        if (to) ctx.navigate(to);
      },
    },
    {
      keys: "t n",
      what: "The next tab",
      group: GROUP_STACK,
      when: onStack,
      reason,
      run: () => {
        const to = tabTarget(ctx.route(), { step: 1 });
        if (to) ctx.navigate(to);
      },
    },
    // fix-kp11-7: ":" is unshifted on a Belgian AZERTY keyboard; the old
    // Ctrl ` needed AltGr there.
    {
      keys: ":",
      what: "Focus the console prompt",
      group: GROUP_CONSOLE,
      when: () => PAGE_KEYS.has("prompt"),
      reason: "Console: only on the Console page",
      run: () => PAGE_KEYS.get("prompt")?.(),
    },
    {
      keys: "r",
      what: "Reveal the chosen secret",
      group: GROUP_SECRETS,
      when: () => PAGE_KEYS.has("reveal"),
      reason: "Reveal: only on a stack's Secrets section",
      run: () => PAGE_KEYS.get("reveal")?.(),
    },
  ];
}
