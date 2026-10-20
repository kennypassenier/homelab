// feat-kpadopt-2 (WP-3): the apps list, read once and shared. The Apps page
// and the command palette's "Open app" group are fed from this one list
// (`GET /data/tiles`), so a launch from the palette opens exactly the apps
// the board shows.

import { fetchJson } from "./dom.js";

/** @typedef {import("./pages/home.js").Tile} Tile */

/** How long a reading stays good for the palette, in ms. */
export const APPS_FRESH_MS = 60_000;

/** @type {{tiles: Tile[], at: number} | null} */
let held = null;
/** @type {Promise<Tile[] | null> | null} */
let reading = null;
/** @type {Set<() => void>} */
const listeners = new Set();

/**
 * The apps as last read, or null before the first answer.
 * @returns {Tile[] | null}
 */
export const appsNow = () => held?.tiles ?? null;

/**
 * Keep a reading the Apps page made itself.
 * @param {Tile[]} tiles
 */
export function shareApps(tiles) {
  held = { tiles, at: Date.now() };
  for (const f of listeners) f();
}

/**
 * Hear when a new reading arrives.
 * @param {() => void} f
 * @returns {() => void}
 */
export function onApps(f) {
  listeners.add(f);
  return () => listeners.delete(f);
}

/**
 * Read the apps unless a recent reading is held; one request at a time.
 * Resolves to null when the host did not answer.
 * @returns {Promise<Tile[] | null>}
 */
export function readApps() {
  if (held && Date.now() - held.at < APPS_FRESH_MS)
    return Promise.resolve(held.tiles);
  reading ??= fetchJson("/data/tiles", "the apps")
    .then((r) => {
      if (!r.ok) return null;
      shareApps(r.body.report?.tiles ?? []);
      return appsNow();
    })
    .catch(() => null)
    .finally(() => {
      reading = null;
    });
  return reading;
}

/**
 * The "Open app" palette commands: one per app that has an address.
 * @returns {import("./intent.js").Command[]}
 */
export const appCommands = () =>
  (appsNow() ?? [])
    .filter((t) => t.url && !t.error)
    .map((t) => ({
      id: `app:${t.host}`,
      group: "Open app",
      label: t.name,
      hint: `${t.stack} · ${t.host}`,
      words: `open launch app apps ${t.host} ${t.stack} ${t.description ?? ""}`,
      run: () => void window.open(t.url, "_blank", "noopener"),
    }));
