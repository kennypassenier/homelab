// The one store (arch-frontend): the fleet snapshot from /data/fleet and the
// host's open questions from /data/asks, kept current by the live channel.
// Pages subscribe and are told on every change.

/**
 * @typedef {{fleet: import("./fleet.js").Fleet | null, up: boolean | null,
 *   link: string, hostVersion: string | null, hostBuild: string | null,
 *   asks: import("./asks.js").Ask[],
 *   asksRead: "loading" | "ready" | "error",
 *   fleetRead: "loading" | "ready" | "error"}} StoreState
 */

/** @type {StoreState} */
const state = {
  fleet: null,
  up: null,
  link: "connecting…",
  hostVersion: null,
  hostBuild: null,
  asks: [],
  asksRead: "loading",
  fleetRead: "loading",
};
/** @type {Set<() => void>} */
const subs = new Set();

/**
 * The live channel itself (feat-kp11demo-14, the connection banner): `open`
 * while events arrive, `down` while it waits to be opened again, `trying`
 * while a new one is being opened (the attempt's number in `attempt`).
 * @typedef {{state: "connecting" | "open" | "down" | "trying",
 *   attempt: number}} Stream
 * @type {Stream}
 */
const stream = { state: "connecting", attempt: 0 };
/** @type {Set<(s: Stream) => void>} */
const streamSubs = new Set();
/** @param {Stream["state"]} next @param {number} [attempt] */
function setStream(next, attempt = stream.attempt) {
  if (stream.state === next && stream.attempt === attempt) return;
  stream.state = next;
  stream.attempt = attempt;
  streamSubs.forEach((f) => f({ ...stream }));
}
/** @type {() => void} */
let retryNow = () => {};

/** @returns {Stream} */
export const streamNow = () => ({ ...stream });

/**
 * Hear the live channel open, drop and try again.
 * @param {(s: Stream) => void} f
 * @returns {() => void} stop
 */
export function onStream(f) {
  streamSubs.add(f);
  return () => streamSubs.delete(f);
}

/** Open the live channel again now (the banner's Retry now). */
export const retryStream = () => retryNow();

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
    state.fleetRead = "ready";
    state.hostVersion = body.host_version ?? null;
    state.hostBuild = body.host_build ?? null;
    if (body.link_error) setLink(false, `host link down: ${body.link_error}`);
    else if (body.host_version) setLink(true, `host ${body.host_version}`);
  } catch (e) {
    if (!state.fleet) state.fleetRead = "error";
    setLink(false, `could not read the fleet (${String(e)})`);
  }
  try {
    const body = await getJson("/data/asks");
    if (body) {
      state.asks = body.asks ?? [];
      state.asksRead = "ready";
    }
  } catch {
    // The questions come again with the next live event; until then the
    // Questions card says it could not read them.
    if (state.asksRead === "loading") state.asksRead = "error";
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
  // ask-kp11be-5: bytes moved so far in a restic backup or restore.
  "action_bytes",
  "action_batch",
  "notification",
  "notifications_read",
  "notify_settings",
  // ask-kp11be-7: a row was snoozed or its snooze ended.
  "inbox_snoozes",
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

/** The wait before the live channel is reopened, in ms: it doubles from
 * the first to the last (fix-live-1). */
export const REOPEN_FIRST_MS = 1000;
export const REOPEN_LAST_MS = 15000;

/**
 * The wait after `failures` failed reopenings in a row.
 * @param {number} failures
 */
export const reopenDelay = (failures) =>
  Math.min(REOPEN_LAST_MS, REOPEN_FIRST_MS * 2 ** Math.max(0, failures));

/** Open the live channel and read the first snapshot. */
export function start() {
  let opens = 0;
  let failures = 0;
  /** @type {ReturnType<typeof setTimeout> | null} */
  let retry = null;
  /** @type {EventSource | null} */
  let events = null;

  /**
   * fix-live-1: an EventSource that got a 502/530 from the edge (the
   * gateway paused for its backup, restarted by its deploy) is CLOSED for
   * good and never retries by itself. Open a fresh one, with backoff.
   */
  const reopen = () => {
    if (retry != null) return;
    if (events && events.readyState !== EventSource.CLOSED) return;
    retry = setTimeout(() => {
      retry = null;
      connect();
    }, reopenDelay(failures++));
  };
  const reopenNow = () => {
    if (events && events.readyState === EventSource.OPEN) return;
    if (retry != null) clearTimeout(retry);
    retry = null;
    connect();
  };

  function connect() {
    events?.close();
    setStream(failures > 0 ? "trying" : "connecting", failures);
    events = new EventSource("/events");
    wire(events);
  }

  /** @param {EventSource} events */
  function wire(events) {
    events.addEventListener("open", () => {
      failures = 0;
      setStream("open", 0);
      if (opens++ > 0) listeners.get("reopened")?.forEach((f) => f(null));
    });
    events.addEventListener("error", () => {
      if (events.readyState !== EventSource.OPEN) setStream("down");
      if (events.readyState === EventSource.CLOSED) reopen();
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
      state.fleetRead = "ready";
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
      state.asksRead = "ready";
      emit();
    });
    events.addEventListener("resync", () => void load());
  }

  // Retry now: open the channel again, or, when it is open and the host link
  // is what is down, read the fleet again to hear whether the link is back.
  retryNow = () => {
    if (events && events.readyState === EventSource.OPEN) void load();
    else reopenNow();
  };
  window.addEventListener("online", reopenNow);
  document.addEventListener("visibilitychange", () => {
    if (!document.hidden) reopenNow();
  });
  connect();
  void load();
}

/** Read the fleet and the questions again (a card's Try again). */
export function rereadStore() {
  if (state.asksRead === "error") state.asksRead = "loading";
  if (state.fleetRead === "error") state.fleetRead = "loading";
  emit();
  return load();
}

/**
 * Replace the open questions (after an answer, before the live event).
 * @param {import("./asks.js").Ask[]} asks
 */
export function setAsks(asks) {
  state.asks = asks;
  state.asksRead = "ready";
  emit();
}
