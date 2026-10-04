// redesign-final (coordinator, 2026-10-04: the bundle-with-a-space bug is a
// fault class): rows are matched, keyed and linked by names — a bundle's
// operation, a stack, an app, a job, an incident, a repository, a snapshot
// tag — and every page compared them its own way ("update kp-soft" never
// met "update-kp-soft"). One normalising key, the same as the Rust side's
// `homelab_core::names::name_key`: lower case, every run of characters
// other than a–z, 0–9, "." and "_" one hyphen, no hyphen at either end.

/**
 * @param {string | null | undefined} name
 * @returns {string}
 */
export function nameKey(name) {
  return String(name ?? "")
    .normalize("NFC")
    .toLowerCase()
    .replace(/[^a-z0-9._]+/g, "-")
    .replace(/^-+|-+$/g, "");
}

/**
 * Whether two names are the same name (by their keys).
 * @param {string | null | undefined} a
 * @param {string | null | undefined} b
 */
export const sameName = (a, b) => nameKey(a) === nameKey(b);
