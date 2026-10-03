// redesign-3.71 secrets (Kenny, 2026-10-03: the approved demo plus "Alle
// drie"): the Secrets page's pure half — what each stack declares as the
// left pane shows it, one secret's row, the host's reason split into why
// and fix, and the store of revealed values that hides each one again
// after 30 seconds. No DOM, no fetch; the clock comes in, so `node --test`
// drives every line with fake timers.

/** How long a revealed value stays on screen. */
export const REVEAL_MS = 30_000;

/**
 * @typedef {{from: string, dest: string, mode?: string,
 *   owner?: string | null, restarts?: string | null}} LatchFile
 * @typedef {{secrets: string[], files: LatchFile[],
 *   unreadable?: string | null}} Declared
 * @typedef {"ok" | "unreadable" | "none"} Kind
 * @typedef {{kind: "env", app: string} |
 *   {kind: "file", from: string, dest: string}} SecretRef
 * @typedef {{key: string, ref: SecretRef, name: string, note: string}} SecretRow
 */

/**
 * What the left pane says about one stack.
 * @param {Declared | null | undefined} d
 * @returns {Kind}
 */
export function stackKind(d) {
  if (!d) return "none";
  if (d.unreadable) return "unreadable";
  return d.secrets.length + d.files.length > 0 ? "ok" : "none";
}

/**
 * The exact count the chip shows: every declared secret and file.
 * @param {Declared | null | undefined} d
 */
export const stackCount = (d) =>
  d && !d.unreadable ? d.secrets.length + d.files.length : 0;

/** @param {number} n @param {string} one @param {string} many */
const plural = (n, one, many) => `${n} ${n === 1 ? one : many}`;

/**
 * The line under a stack's name in the left pane.
 * @param {Declared | null | undefined} d
 */
export function stackLine(d) {
  const k = stackKind(d);
  if (k === "unreadable") return "stack file unreadable";
  if (k === "none" || !d) return "declares none";
  return `${plural(d.secrets.length, "secret", "secrets")} · ${plural(d.files.length, "file", "files")}`;
}

/**
 * The host's (or the working copy's) "why :: fix" as two sentences.
 * @param {string} text
 * @returns {{why: string, fix: string}}
 */
export function splitReason(text) {
  const at = text.indexOf(" :: ");
  if (at < 0) return { why: text.trim(), fix: "" };
  return { why: text.slice(0, at).trim(), fix: text.slice(at + 4).trim() };
}

/**
 * One row per declared secret, latch secrets first, then files. `key` is
 * the row's Live view name: `<stack>/<app>/.env` or `<stack>/<from>`, the
 * same path latch keeps it under.
 * @param {string} stack
 * @param {Declared} d
 * @returns {SecretRow[]}
 */
export function secretRows(stack, d) {
  return [
    ...d.secrets.map((app) => ({
      key: `${stack}/${app}/.env`,
      ref: /** @type {SecretRef} */ ({ kind: "env", app }),
      name: `${app}/.env`,
      note: `latch secret · injected into ${app}'s environment`,
    })),
    ...d.files.map((f) => ({
      key: `${stack}/${f.from}`,
      ref: /** @type {SecretRef} */ ({
        kind: "file",
        from: f.from,
        dest: f.dest,
      }),
      name: f.from,
      note: `latch file · written to ${f.dest}`,
    })),
  ];
}

/**
 * The revealed values, each hidden again `ttlMs` after it was shown. A
 * value lives only here, in memory, never in the URL, storage or the DOM
 * once hidden; `clear()` on leaving the page drops them all.
 * @param {{onChange: () => void, ttlMs?: number,
 *   setTimer?: (f: () => void, ms: number) => any,
 *   clearTimer?: (t: any) => void, now?: () => number}} opts
 */
export function createReveals(opts) {
  const ttl = opts.ttlMs ?? REVEAL_MS;
  const setTimer = opts.setTimer ?? ((f, ms) => setTimeout(f, ms));
  const clearTimer = opts.clearTimer ?? ((t) => clearTimeout(t));
  const now = opts.now ?? (() => Date.now());
  /** @type {Map<string, {value: string, until: number, timer: any}>} */
  const shown = new Map();
  const drop = (/** @type {string} */ key) => {
    const s = shown.get(key);
    if (!s) return false;
    clearTimer(s.timer);
    shown.delete(key);
    return true;
  };
  return {
    /** Show `value` for `key` for the next `ttl` ms. @param {string} key @param {string} value */
    reveal(key, value) {
      drop(key);
      const timer = setTimer(() => {
        if (shown.delete(key)) opts.onChange();
      }, ttl);
      shown.set(key, { value, until: now() + ttl, timer });
      opts.onChange();
    },
    /** @param {string} key */
    hide(key) {
      if (drop(key)) opts.onChange();
    },
    hideAll() {
      if (shown.size === 0) return;
      for (const k of [...shown.keys()]) drop(k);
      opts.onChange();
    },
    /** Drop every value without telling anyone (the page is going). */
    clear() {
      for (const k of [...shown.keys()]) drop(k);
    },
    /** @param {string} key @returns {string | null} */
    value: (key) => shown.get(key)?.value ?? null,
    /** Milliseconds until `key` hides, 0 when hidden. @param {string} key */
    left: (key) => Math.max(0, (shown.get(key)?.until ?? 0) - now()),
    get size() {
      return shown.size;
    },
    ttl,
  };
}
