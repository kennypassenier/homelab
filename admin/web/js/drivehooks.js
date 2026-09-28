// Milestone follow (feat-platform-10): how the Live view replay reaches the
// edit forms' own dialogs and wizards. A page or dialog that Claude can
// drive registers a small handle here while it is on screen (the stack's
// settings tab, its firewall tab, the plan dialog, the rule dialog, the
// new-stack wizard, the host settings page, the batch and roll back
// dialogs); the replay (editdrive.js) finds it by name and plays the step
// on the real thing, never on a copy.
//
// `driven()` says whether this tab is playing Claude's steps right now: a
// dialog's final press (commit, write host.toml, run the batch) then runs
// on the dashboard's server, once, and the tab only shows what it did.

/** @type {Map<string, any>} */
const handles = new Map();
let on = false;

/** Whether this tab plays Claude's steps now (Live view on, Claude driving). */
export const driven = () => on;

/** @param {boolean} v */
export function setDriven(v) {
  on = v;
}

/**
 * Make a handle findable while its page or dialog is on screen.
 * @param {string} key
 * @param {any} handle
 * @returns {() => void} unregister
 */
export function register(key, handle) {
  handles.set(key, handle);
  return () => {
    if (handles.get(key) === handle) handles.delete(key);
  };
}

/** @param {string} key */
export const handle = (key) => handles.get(key) ?? null;

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/**
 * Wait until `f` gives something (a page mounts after its fetch).
 * @template T
 * @param {() => T | null | undefined | false} f
 * @param {number} [ms]
 * @returns {Promise<T | null>}
 */
export async function waitFor(f, ms = 5000) {
  const end = Date.now() + ms;
  for (;;) {
    const v = f();
    if (v) return v;
    if (Date.now() > end) return null;
    await sleep(40);
  }
}

/**
 * @param {string} key
 * @param {number} [ms]
 */
export const waitHandle = (key, ms) => waitFor(() => handle(key), ms);
