// The one store (arch-frontend): the fleet snapshot from /data/fleet and the
// host's open questions from /data/asks, kept current by the live channel.
// Pages subscribe and are told on every change.

/**
 * @typedef {{fleet: import("./fleet.js").Fleet | null, up: boolean | null,
 *   link: string, hostVersion: string | null, hostBuild: string | null,
 *   asks: import("./asks.js").Ask[]}} StoreState
 */

/** @type {StoreState} */
const state = {
  fleet: null,
  up: null,
  link: "connecting…",
  hostVersion: null,
  hostBuild: null,
  asks: [],
};
/** @type {Set<() => void>} */
const subs = new Set();

const emit = () => subs.forEach((f) => f());

/** @returns {StoreState} */
export const current = () => state;

/**
 * @param {() => void} f
 * @returns {() => void} unsubscribe
 */
export function subscribe(f) {
  subs.add(f);
  return () => subs.delete(f);
}

/** @param {boolean} up @param {string} text */
function setLink(up, text) {
  state.up = up;
  state.link = text;
}

/**
 * @param {string} url
 * @returns {Promise<any>} the JSON body, or null
 */
async function getJson(url) {
  const r = await fetch(url, { headers: { accept: "application/json" } });
  if (r.redirected && new URL(r.url).pathname === "/login") {
    location.assign("/login");
    return null;
  }
  if (!r.ok) throw new Error(`HTTP ${r.status}`);
  return r.json();
}

async function load() {
  try {
    const body = await getJson("/data/fleet");
    if (!body) return;
    state.fleet = body.fleet;
    state.hostVersion = body.host_version ?? null;
    state.hostBuild = body.host_build ?? null;
    if (body.link_error) setLink(false, `host link down: ${body.link_error}`);
    else if (body.host_version) setLink(true, `host ${body.host_version}`);
  } catch (e) {
    setLink(false, `could not read the fleet (${String(e)})`);
  }
  try {
    const body = await getJson("/data/asks");
    if (body) state.asks = body.asks ?? [];
  } catch {
    // The questions come again with the next live event.
  }
  emit();
}

/**
 * The live events of milestone act, handed to their own listeners (act.js)
 * rather than to every page: a log line must not redraw the fleet table.
 */
export const ACT_EVENTS = /** @type {const} */ ([
  "action",
  "action_log",
  "action_progress",
  "action_batch",
  "notification",
  "notifications_read",
  "notify_settings",
  "schedules",
  "resync",
  // Milestone edit: the working copy moved, host.toml was written.
  "repo",
  "host_settings",
  // fix-120 (per-machine tokens, owner decision 2026-10-01): a token was
  // issued or revoked.
  "tokens",
  // Milestone follow: Claude drove one step (feat-platform-10).
  "drive",
  // TUI parity: every host line, the byte counters, the newest release.
  "host_log",
  "transfer",
  "release",
  // slow-reads: a slow read (Today, the fleet check, the doctor) finished;
  // a page fetches it by id (slowread.js).
  "slow_read",
  "link",
  // Not the server's: the live channel opened again after it dropped (the
  // dashboard restarted, a release installed). chassis sends `resync` only
  // to a browser that had heard an event before; this comes on every
  // reconnect.
  "reopened",
]);

/** @type {Map<string, Set<(data: any) => void>>} */
const listeners = new Map();

/**
 * Hear one live event, its JSON parsed.
 * @param {(typeof ACT_EVENTS)[number]} name
 * @param {(data: any) => void} f
 * @returns {() => void} stop
 */
export function listen(name, f) {
  let set = listeners.get(name);
  if (!set) {
    set = new Set();
    listeners.set(name, set);
  }
  set.add(f);
  return () => set.delete(f);
}

/** Open the live channel and read the first snapshot. */
export function start() {
  const events = new EventSource("/events");
  let opens = 0;
  events.addEventListener("open", () => {
    if (opens++ > 0) listeners.get("reopened")?.forEach((f) => f(null));
  });
  for (const name of ACT_EVENTS)
    events.addEventListener(name, (e) => {
      /** @type {any} */
      let data = null;
      try {
        data = JSON.parse(/** @type {MessageEvent} */ (e).data || "null");
      } catch {
        return;
      }
      listeners.get(name)?.forEach((f) => f(data));
    });
  events.addEventListener("fleet", (e) => {
    state.fleet = JSON.parse(/** @type {MessageEvent} */ (e).data).fleet;
    emit();
  });
  events.addEventListener("link", (e) => {
    const d = JSON.parse(/** @type {MessageEvent} */ (e).data);
    if (d.up) {
      state.hostVersion = d.host_version ?? null;
      state.hostBuild = d.host_build ?? null;
    }
    setLink(
      d.up,
      d.up ? `host ${d.host_version}` : `host link down: ${d.error}`,
    );
    emit();
  });
  events.addEventListener("asks", (e) => {
    state.asks = JSON.parse(/** @type {MessageEvent} */ (e).data).asks ?? [];
    emit();
  });
  events.addEventListener("resync", () => void load());
  void load();
}

/**
 * Replace the open questions (after an answer, before the live event).
 * @param {import("./asks.js").Ask[]} asks
 */
export function setAsks(asks) {
  state.asks = asks;
  emit();
}
