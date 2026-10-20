// redesign-3.71 secrets (Kenny, 2026-10-03: the approved demo plus "Alle
// drie"): the Secrets page's pure half — what each stack declares as the
// left pane shows it, one secret's row, the host's reason split into why
// and fix, and the store of revealed values that hides each one again
// after 30 seconds. No DOM, no fetch; the clock comes in, so `node --test`
// drives every line with fake timers.

/**
 * @typedef {{from: string, dest: string, mode?: string,
 *   owner?: string | null, restarts?: string | null}} LatchFile
 * @typedef {{secrets: string[], files: LatchFile[], restart_order?: string[],
 *   unreadable?: string | null}} Declared
 *   `restart_order`: the stack's native units in the order a deploy restarts
 *   them (absent from a dashboard before it was added)
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
