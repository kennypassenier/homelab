// Pure helpers for the stack page's tabs (feat-stacks-1): which history
// entries, incidents and manual checks belong to one stack.

/** The host's operations on one stack, as the first word(s) of their name. */
const VERBS = [
  "deploy",
  "backup",
  "restore",
  "update",
  "destroy",
  "resize",
  "apply",
  "forget",
  "wipe",
  "enable",
  "disable",
  "adopt",
  "install-native",
  "backup-native",
  "update-native",
  "release-update-native",
  "rollback-native",
  "guards",
];

/**
 * Whether an operation's name is about `stack`. The host names them
 * "<verb>-<stack>" ("deploy-media", "backup-kp-soft"), older entries
 * "<verb> <stack>". With the entry's own `label` the verb is known;
 * without it (an incident bundle) it must be one of the stack verbs, so
 * "self-update" is never about a stack called "update".
 * @param {string | null | undefined} subject
 * @param {string} stack
 * @param {string} [label]
 */
export function aboutStack(subject, stack, label) {
  if (!subject) return false;
  const verbs = label ? [label] : VERBS;
  return verbs.some(
    (v) => subject === `${v}-${stack}` || subject === `${v} ${stack}`,
  );
}

/**
 * The history entries of one stack (operations only: a nightly phase is
 * about the whole fleet).
 * @param {import("./activity.js").Entry[]} entries
 * @param {string} stack
 */
export function stackEntries(entries, stack) {
  return entries.filter(
    (e) => e && e.kind === "op" && aboutStack(e.subject, stack, e.label),
  );
}

/**
 * The incident bundles of one stack.
 * @param {string[]} names
 * @param {string} stack
 */
export function stackIncidents(names, stack) {
  return names.filter((n) => aboutStack(n.replace(/^\d+-/, ""), stack));
}

/**
 * The manual checks of one stack.
 * @param {import("./checks.js").Check[]} checks
 * @param {string} stack
 */
export function stackChecks(checks, stack) {
  return checks.filter((c) => c.record.stack === stack);
}
