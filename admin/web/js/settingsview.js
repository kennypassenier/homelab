// redesign-settings (3.71.0, Kenny's approved demo settings.html): the
// Settings page's words, filters and counts, pure so they are tested
// without a browser.

/**
 * @typedef {import("./editforms.js").HostField} HostField
 * @typedef {"all" | "changed" | "here"} Show
 */

/**
 * Who changes a key, in the demo's words: the short label beside the
 * "when" dot, and the sentence the "How to…" box and its tip say.
 * `browser` keys need no word: they are simply edited here.
 */
export const ACCESS_WORDS = /** @type {const} */ ({
  confirm: [
    "Here, with the key typed",
    "Changing it can take routes or backups down, so you type its key to confirm.",
  ],
  secret: ["ssh only · secret", "A secret the dashboard never sees."],
  ssh_only: [
    "ssh only · safety policy",
    "A policy the dashboard's token must not loosen.",
  ],
  locked: [
    "ssh only · cuts the dashboard off",
    "Changing it from here would cut this dashboard off.",
  ],
  host_held: ["kept by the host", "Generated and kept by the host itself."],
  dashboard_secret: [
    "Here · write-only secret",
    "You can replace it; nobody can read it back.",
  ],
});

/**
 * The words for a field's access, or null for a plain browser key.
 * @param {HostField} f
 * @returns {readonly [string, string] | null}
 */
export const accessWords = (f) =>
  f.access in ACCESS_WORDS
    ? ACCESS_WORDS[/** @type {keyof typeof ACCESS_WORDS} */ (f.access)]
    : null;

/** A group's anchor: "Nightly round and backups" → "nightly-round-and-backups". @param {string} s */
export const slug = (s) =>
  s
    .toLowerCase()
    .replace(/[^a-z0-9]+/g, "-")
    .replace(/^-|-$/g, "");

/**
 * The groups in the order the host lists their keys.
 * @param {HostField[]} fields
 */
export const groupsOf = (fields) => [...new Set(fields.map((f) => f.group))];

/**
 * Whether a field is shown under the search and the "Show" choice.
 * @param {HostField} f
 * @param {string} q lower-case
 * @param {Show} show
 * @param {(f: HostField) => boolean} editable
 * @param {(key: string) => boolean} staged
 */
export function fieldShown(f, q, show, editable, staged) {
  if (
    q &&
    ![f.label, f.key, f.help, f.default].join(" ").toLowerCase().includes(q)
  )
    return false;
  if (show === "changed") return f.set || staged(f.key);
  if (show === "here") return editable(f);
  return true;
}

/**
 * A group's chip: "17 keys", "3 of 17 keys", with " · 2 changed".
 * @param {number} shown
 * @param {number} total
 * @param {number} changed
 */
export const groupChip = (shown, total, changed) =>
  `${shown === total ? total : `${shown} of ${total}`} key${total === 1 ? "" : "s"}${changed ? ` · ${changed} changed` : ""}`;

/**
 * The toolbar's count: "64 settings", or "5 of 64 settings" while a search
 * or a Show choice narrows them.
 * @param {number} shown
 * @param {number} total
 * @param {boolean} narrowed
 */
export const foundText = (shown, total, narrowed) =>
  narrowed ? `${shown} of ${total} settings` : `${total} settings`;

/**
 * The header's staged chip and primary button.
 * @param {number} n
 */
export const stagedWords = (n) => ({
  chip: n ? `${n} change${n === 1 ? "" : "s"} staged` : "nothing staged",
  write: n ? `Check and write ${n}…` : "Check and write",
});

/**
 * The working copy against its remote, in words.
 * @param {{present: boolean, behind: number, unpushed: unknown[],
 *   dirty: unknown[], error: string | null}} r
 * @returns {{tone: "ok" | "warn" | "bad", word: string}}
 */
export function repoState(r) {
  if (!r.present) return { tone: "bad", word: "not cloned yet" };
  if (r.error) return { tone: "bad", word: "the last fetch failed" };
  if (r.dirty.length)
    return { tone: "bad", word: "files differ from the commit" };
  if (r.unpushed.length || r.behind)
    return { tone: "warn", word: "differs from the remote" };
  return { tone: "ok", word: "in step with the remote" };
}

/**
 * The remote, shortened to its last two parts for the chip (the full one
 * is its tooltip): "git@github.com:me/stacks.git" → "…me/stacks.git".
 * @param {string} remote
 */
export function shortRemote(remote) {
  const parts = remote.split(/[/:]/).filter(Boolean);
  return parts.length > 2 ? `…/${parts.slice(-2).join("/")}` : remote;
}

/**
 * The section a `?section=` names: the two dashboard sections, or a
 * host.toml group by its slug; null when it names none of them.
 * @param {string | null} section
 * @param {string[]} groups
 * @returns {string | null} the element id
 */
export function sectionTarget(section, groups) {
  if (!section) return null;
  if (section === "sign-in" || section === "signin") return "signin";
  if (section === "repo" || section === "working-copy") return "wc";
  return groups.map(slug).find((g) => g === section) ?? null;
}

/** Where the tab keeps its staged host.toml changes (redesign-config-5). */
export const STAGED_STORE = "homelab:settings-staged";

/**
 * The staged changes as the tab keeps them while you look elsewhere
 * (redesign-config-5): key → value, with every secret left out (a secret
 * never goes to browser storage; leaving the page drops it, and says so).
 * @template {{key: string}} F
 * @param {Map<string, {field: F, value: unknown}>} changes
 * @param {(f: F) => boolean} secret
 * @returns {string}
 */
export function keepStaged(changes, secret) {
  /** @type {Record<string, unknown>} */
  const out = {};
  for (const [k, { field, value }] of changes)
    if (!secret(field)) out[k] = value;
  return JSON.stringify(out);
}

/**
 * The kept changes back, for the keys the host still reads; anything
 * broken restores nothing.
 * @param {string | null} raw
 * @param {{key: string}[]} fields
 * @returns {Record<string, unknown>}
 */
export function restoreStaged(raw, fields) {
  if (!raw) return {};
  try {
    const v = JSON.parse(raw);
    if (!v || typeof v !== "object" || Array.isArray(v)) return {};
    /** @type {Record<string, unknown>} */
    const out = {};
    for (const [k, x] of Object.entries(v))
      if (fields.some((f) => f.key === k)) out[k] = x;
    return out;
  } catch {
    return {};
  }
}

/**
 * One message for every read the page lost at once (redesign-config-11):
 * the dashboard's line to the host carries all of them, so one cause
 * shows once, with one Try again.
 * @param {{what: string, error: {why: string, fix?: string | null}}[]} fails
 * @returns {{title: string, why: string, fix: string | null} | null}
 */
export function failSummary(fails) {
  if (!fails.length) return null;
  const w = fails.map((f) => f.what);
  const list =
    w.length === 1 ? w[0] : `${w.slice(0, -1).join(", ")} and ${w.at(-1)}`;
  return {
    title: `Could not read ${list}`,
    why: fails[0].error.why,
    fix: fails[0].error.fix ?? null,
  };
}
