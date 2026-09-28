// The one store (arch-frontend): the fleet snapshot from /data/fleet, kept
// current by the live channel. Pages subscribe and are told on every change.

/**
 * @typedef {{fleet: import("./fleet.js").Fleet | null, up: boolean | null,
 *   link: string}} StoreState
 */

/** @type {StoreState} */
const state = { fleet: null, up: null, link: "connecting…" };
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

async function load() {
  try {
    const r = await fetch("/data/fleet", {
      headers: { accept: "application/json" },
    });
    if (r.redirected && new URL(r.url).pathname === "/login") {
      location.assign("/login");
      return;
    }
    if (!r.ok) {
      setLink(false, `could not read the fleet (HTTP ${r.status})`);
      emit();
      return;
    }
    const body = await r.json();
    state.fleet = body.fleet;
    if (body.link_error) setLink(false, `host link down: ${body.link_error}`);
    else if (body.host_version) setLink(true, `host ${body.host_version}`);
  } catch {
    setLink(false, "the dashboard did not answer");
  }
  emit();
}

/** Open the live channel and read the first snapshot. */
export function start() {
  const events = new EventSource("/events");
  events.addEventListener("fleet", (e) => {
    state.fleet = JSON.parse(/** @type {MessageEvent} */ (e).data).fleet;
    emit();
  });
  events.addEventListener("link", (e) => {
    const d = JSON.parse(/** @type {MessageEvent} */ (e).data);
    setLink(
      d.up,
      d.up ? `host ${d.host_version}` : `host link down: ${d.error}`,
    );
    emit();
  });
  events.addEventListener("resync", () => void load());
  void load();
}
