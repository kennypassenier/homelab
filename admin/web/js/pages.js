// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the page registry, the
// single list every navigation renders from. `main.rs` registers the
// dashboard's own pages plus the kit's (Status, Clients, Passkeys); this
// module reads `GET /api/kit/pages` once at boot and keeps it, so the nav
// bar and the command palette (commands.js) never carry a second,
// hand-written list that can drift from what the server registered.

import { fetchJson } from "./dom.js";

/**
 * @typedef {{id: string, title: string, path: string, group: string | null,
 *   order: number, nav: boolean, source: "kit" | "app",
 *   render: "kit" | "app"}} RegPage
 * @typedef {{app: string, brand: {title: string, href: string}, home: string,
 *   pages: RegPage[]}} PageSet
 */

/** @type {PageSet | null} */
let current = null;
/** @type {Set<() => void>} */
const listeners = new Set();

/** The registry as last read, or `null` before the first answer. */
export function pages() {
  return current;
}

/**
 * Set the registry directly (what `loadPages` does once it has an answer;
 * a test primes the module the same way, without a network call).
 * @param {PageSet} p
 */
export function setPages(p) {
  current = p;
  for (const fn of listeners) fn();
}

/**
 * @param {() => void} fn called once more whenever the registry changes
 * @returns {() => void} stop listening
 */
export function subscribePages(fn) {
  listeners.add(fn);
  return () => listeners.delete(fn);
}

/**
 * Read `GET /api/kit/pages` and keep the answer. Safe to call more than
 * once (e.g. a retry); each answer replaces the one before it.
 * @param {AbortSignal} [signal]
 */
export async function loadPages(signal) {
  const r = await fetchJson("/api/kit/pages", "the page list", signal);
  if (r.ok) setPages(r.body);
  return r;
}

/**
 * A page of the registry by id, or `undefined` before it has loaded.
 * @param {string} id
 */
export function pageById(id) {
  return current?.pages.find((p) => p.id === id);
}
