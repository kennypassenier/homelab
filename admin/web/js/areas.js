// feat-shell-1 (redesign 3.71.0, Kenny approved 2026-10-03): the
// dashboard's information architecture as data. Six areas, ordered by how
// often and in which order a person needs them (FLOWS.md §1.1):
//
//   Apps · Inbox · Stacks · Activity │ Backups · System
//
// The nav bar, the phone tab bar and its More sheet, the breadcrumbs, the
// System landing and the command palette's "Go to" all read this one list,
// so no surface carries a second, hand-kept copy. The page registry
// (`GET /api/kit/pages`, admin/src/main.rs) registers the same six as its
// only `nav: true` pages, in this order; a test holds the two equal.

/**
 * @typedef {{id: string, label: string, href: string, what: string,
 *   rare: boolean, phone: "tab" | "more", keys: string}} Area
 * @typedef {{id: string, label: string, href: string, what: string,
 *   area: string, words: string}} SubPage
 */

/** @type {readonly Area[]} */
export const AREAS = Object.freeze([
  {
    id: "home",
    label: "Apps",
    href: "/apps",
    what: "Open the apps the homelab runs",
    rare: false,
    phone: "tab",
    keys: "g u",
  },
  {
    id: "inbox",
    label: "Inbox",
    href: "/inbox",
    what: "Everything waiting for a person, worst first",
    rare: false,
    phone: "tab",
    keys: "g i",
  },
  {
    id: "overview",
    label: "Stacks",
    href: "/stacks",
    what: "Every stack: its state, and everything you can do to it",
    rare: false,
    phone: "tab",
    keys: "g s",
  },
  {
    id: "activity",
    label: "Activity",
    href: "/activity",
    what: "What runs now, what ran, who ran it, and what is planned",
    rare: false,
    phone: "tab",
    keys: "g a",
  },
  {
    id: "backups",
    label: "Backups",
    href: "/backups",
    what: "Every snapshot, every night, and Restore",
    rare: true,
    phone: "more",
    keys: "g b",
  },
  {
    id: "system",
    label: "System",
    href: "/system",
    what: "The host, metrics, the map and settings; rarely needed",
    rare: true,
    phone: "more",
    keys: "g y",
  },
]);

/**
 * The pages that live inside an area, beyond the area's own page (FLOWS.md
 * §2 "Clustering"). `words` are what a person may type for it in the
 * palette.
 * @type {readonly SubPage[]}
 */
export const SUB_PAGES = Object.freeze([
  {
    id: "running",
    label: "Running now",
    href: "/activity?view=running",
    what: "The jobs running on the host right now",
    area: "activity",
    words: "jobs running queue",
  },
  {
    id: "planned",
    label: "Planned",
    href: "/activity?view=planned",
    what: "Schedules: what runs when, and the nightly round",
    area: "activity",
    words: "schedules schedule cron nightly timer",
  },
  {
    id: "host-log",
    label: "Host log",
    href: "/activity?view=host-log",
    what: "The host's own log of what it did",
    area: "activity",
    words: "log journal operations live",
  },
  {
    id: "coverage",
    label: "Nightly coverage",
    href: "/backups?section=coverage",
    what: "Which stack was backed up which night, as a calendar",
    area: "backups",
    words: "calendar nights backup",
  },
  {
    id: "removed",
    label: "Removed stacks",
    href: "/backups?section=removed",
    what: "What the backups kept of stacks that are gone",
    area: "backups",
    words: "retired removed old destroyed",
  },
  {
    id: "host",
    label: "Host",
    href: "/host",
    what: "The Proxmox host: load, disk and host-wide actions",
    area: "system",
    words: "proxmox pve disk load cpu memory",
  },
  {
    id: "metrics",
    label: "Metrics",
    href: "/charts",
    what: "System and traffic charts over time",
    area: "system",
    words: "charts graphs traffic prometheus",
  },
  {
    id: "fleetview",
    label: "Map",
    href: "/map",
    what: "Topology, capacity and disk growth of the fleet",
    area: "system",
    words: "fleet view topology capacity disk growth dependencies",
  },
  {
    id: "firewall",
    label: "Firewall",
    href: "/firewall",
    what: "Every firewall rule of every stack",
    area: "system",
    words: "rules ports",
  },
  {
    id: "settings",
    label: "Host settings",
    href: "/settings",
    what: "host.toml, tokens and how you sign in",
    area: "system",
    words: "settings config host.toml tokens",
  },
  {
    id: "sign-in",
    label: "Sign-in",
    href: "/settings?section=sign-in",
    what: "Passkeys you sign in to this dashboard with",
    area: "system",
    words: "passkeys passkey login access",
  },
  {
    id: "presets",
    label: "Presets",
    href: "/presets",
    what: "The catalogue a new stack starts from",
    area: "system",
    words: "templates catalog catalogue",
  },
  {
    id: "notifications",
    label: "Notification rules",
    href: "/system/notifications",
    what: "What is pushed, the daily digest, snooze and per-stack mute",
    area: "system",
    words: "notifications push digest mute snooze",
  },
  {
    id: "shell",
    label: "Console",
    href: "/console",
    what: "A shell on the host or in a stack",
    area: "system",
    words: "shell exec command terminal",
  },
]);

/** The route pages each area owns besides its own (for the current area). */
const AREA_OF_PAGE = /** @type {Record<string, string>} */ ({
  landing: "home",
  home: "home",
  inbox: "inbox",
  overview: "overview",
  stack: "overview",
  activity: "activity",
  backups: "backups",
  system: "system",
  host: "system",
  metrics: "system",
  fleetview: "system",
  firewall: "system",
  settings: "system",
  presets: "system",
  notifications: "system",
  shell: "system",
});

/**
 * The area a route belongs to, or null (a page not found).
 * @param {import("./router.js").Route} r
 * @returns {string | null}
 */
export function areaOf(r) {
  // redesign-flows-11: the Update flow belongs to Stacks when it is one
  // stack's, to the Inbox when it is every app's.
  if (r.page === "update") {
    const q = new URLSearchParams(globalThis.location?.search ?? "");
    return q.get("stack") ? "overview" : "inbox";
  }
  return AREA_OF_PAGE[r.page] ?? null;
}

/** @param {string} id */
export const areaById = (id) => AREAS.find((a) => a.id === id) ?? null;

/**
 * The breadcrumb trail of a location, ending at the page itself:
 * `Stacks / gateway / Logs`, `System / Map`, `Activity / Planned`. A
 * crumb with an `href` is a link up; the last one has none.
 * @param {import("./router.js").Route} r
 * @param {string} pathname the location's path
 * @param {string} search the location's query string
 * @param {{tabLabel?: (tab: string) => string}} [opts]
 * @returns {{label: string, href?: string}[]}
 */
export function crumbs(r, pathname, search, opts = {}) {
  const id = areaOf(r);
  const area = id ? areaById(id) : null;
  if (!area) return [];
  if (r.page === "stack") {
    const hub = `/stacks/${encodeURIComponent(r.name)}`;
    if (r.tab === "overview")
      return [{ label: area.label, href: area.href }, { label: r.name }];
    return [
      { label: area.label, href: area.href },
      { label: r.name, href: hub },
      { label: opts.tabLabel?.(r.tab) ?? r.tab },
    ];
  }
  const params = new URLSearchParams(search);
  const path = pathname.replace(/\/+$/, "") || "/";
  // The most specific sub-page this location is (most query keys matched).
  const sub = SUB_PAGES.map((s) => ({ s, u: new URL(s.href, "http://x") }))
    .filter(
      ({ u }) =>
        u.pathname === path &&
        [...u.searchParams].every(([k, v]) => params.get(k) === v),
    )
    .sort((a, b) => b.u.searchParams.size - a.u.searchParams.size)[0]?.s;
  if (sub)
    return [{ label: area.label, href: area.href }, { label: sub.label }];
  return [{ label: area.label }];
}

/**
 * The phone tab bar: the four frequent areas within thumb reach, then
 * More (FLOWS.md §1.1 "Phone").
 */
export const phoneTabs = () => AREAS.filter((a) => a.phone === "tab");

/** The More sheet's areas. */
export const moreAreas = () => AREAS.filter((a) => a.phone === "more");
