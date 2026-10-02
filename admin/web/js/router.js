import { setParams } from "./urlstate.js";

// The client-side router's pure half (arch-frontend): a path in, a page
// out. chassis answers index.html for every extensionless path the kit
// does not claim (`/api`, `/static`, `/login`, `/logout`, `/clients`,
// `/passkeys`, `/healthz`, `/readyz`, `/metrics` stay the kit's; with
// `kit_pages_in_webapp` the kit claims no GET page at `/status`,
// `/clients` or `/passkeys` either, so this app's fallback serves them
// too). feat-overview-8: every page and every stack tab has its own path;
// what a page filters on lives in the query string (urlstate.js), so any
// state a person can see is a link they can keep.
//
// nav-decisions (chassis-rs 3.1.0, Kenny 2026-10-01): since every path now
// lives at the root (`/app/…` from before is a 308 the kit answers, not a
// route this module parses), and the nav bar and the command palette
// (commands.js) render from the page registry (`GET /api/kit/pages`, see
// pages.js) instead of a hand-written list here. This module keeps only
// what the registry cannot know: which path belongs to which SPA page
// (`ROUTES`), the stack sub-router, and the client-side fallback titles
// for a page the registry has not answered yet.

/** The stack page's tabs (feat-stacks-1), in order. */
export const STACK_TABS = /** @type {const} */ ([
  { tab: "overview", label: "Overview" },
  { tab: "apps", label: "Apps" },
  { tab: "history", label: "History" },
  { tab: "logs", label: "Logs" },
  { tab: "checks", label: "Checks" },
  // Milestone edit (feat-stacks-2, feat-firewall-1).
  { tab: "settings", label: "Settings" },
  { tab: "firewall", label: "Firewall" },
]);

/** @typedef {(typeof STACK_TABS)[number]["tab"]} StackTab */

/**
 * @typedef {{page: "overview"} | {page: "home"} | {page: "health"} | {page: "metrics"} | {page: "host"} |
 *   {page: "stack", name: string, tab: StackTab} |
 *   {page: "activity"} | {page: "jobs"} | {page: "schedules"} |
 *   {page: "notifications"} | {page: "firewall"} | {page: "settings"} |
 *   {page: "backups"} | {page: "retired"} | {page: "secrets"} |
 *   {page: "log"} | {page: "shell"} | {page: "apply"} |
 *   {page: "presets"} |
 *   {page: "fleetview"} | {page: "backupcalendar"} |
 *   {page: "status"} | {page: "clients"} | {page: "passkeys"} |
 *   {page: "start"} | {page: "today"} | {page: "doctor"} | {page: "checks"} |
 *   {page: "charts"} | {page: "traffic"} | {page: "timeline"} |
 *   {page: "home-legacy"} |
 *   {page: "notfound", path: string}} Route
 * The last group (start, today, doctor, checks, charts, traffic, timeline)
 * are 2026-09-30's retired addresses: `route()` still names them, so an old
 * link or a Live view script naming one is recognised, but `main.js`
 * redirects every one of them before it ever mounts a page for them (see
 * `redirectFor`). `status`, `clients` and `passkeys` are the kit's own
 * pages, drawn by this app since `kit_pages_in_webapp()`.
 */

/**
 * Every address this app answers itself, by its route's `page`. The page
 * registry (pages.js) carries each one's title, group and order; this
 * table only says which URL is which page, for `route()` and `pageTitle`'s
 * fallback before the registry has answered.
 * @type {Record<string, Route["page"]>}
 */
const PATH_TO_PAGE = {
  "": "home",
  overview: "overview",
  health: "health",
  metrics: "metrics",
  host: "host",
  activity: "activity",
  jobs: "jobs",
  schedules: "schedules",
  notifications: "notifications",
  firewall: "firewall",
  backups: "backups",
  retired: "retired",
  secrets: "secrets",
  settings: "settings",
  log: "log",
  shell: "shell",
  apply: "apply",
  presets: "presets",
  fleetview: "fleetview",
  backupcalendar: "backupcalendar",
  status: "status",
  clients: "clients",
  passkeys: "passkeys",
  // 2026-09-30: retired addresses, kept parseable for old links and for
  // Live view's known_page (core::drive checks formspec.json's page list,
  // not this table); `redirectFor` sends every one of them on to its new
  // home before main.js ever mounts a page for them.
  start: "start",
  today: "today",
  doctor: "doctor",
  checks: "checks",
  charts: "charts",
  traffic: "traffic",
  timeline: "timeline",
  // Pre-3.1.0 address for the tile page, now the root: kept so an old
  // bookmark still lands somewhere (`redirectFor` sends it on to `/`).
  home: "home-legacy",
};

/**
 * The addresses a driven `goto` may land on (core::drive's `known_page`),
 * i.e. every current page of `PATH_TO_PAGE` without the retired or
 * pre-3.1.0 ones `redirectFor` sends on elsewhere. A web test holds this
 * equal to `formspec.json`'s `pages` (follow.test.js), which `core::drive`
 * reads on the server.
 */
export const DRIVABLE_PATHS = Object.keys(PATH_TO_PAGE).filter(
  (k) =>
    ![
      "start",
      "today",
      "doctor",
      "checks",
      "charts",
      "traffic",
      "timeline",
      "home",
    ].includes(k),
);

/** Fallback titles for a page before the registry has answered. */
const FALLBACK_TITLE = /** @type {Record<string, string>} */ ({
  home: "Apps",
  overview: "Overview",
  health: "Health",
  metrics: "Metrics",
  activity: "Activity",
  host: "Host",
  log: "Live log",
  jobs: "Jobs",
  apply: "Apply",
  schedules: "Schedules",
  firewall: "Firewall",
  backups: "Backups",
  retired: "Retired",
  secrets: "Secrets",
  settings: "Settings",
  fleetview: "Fleet view",
  backupcalendar: "Backup calendar",
  notifications: "Notifications",
  shell: "Shell",
  presets: "Presets",
  status: "Status",
  clients: "Clients",
  passkeys: "Passkeys",
});

/**
 * The retired (or pre-3.1.0) path's replacement, its query string kept
 * and extended so the merged page opens on the right block or tab; `null`
 * for a path that is not retired.
 * @param {Route} r
 * @param {string} search e.g. location.search
 * @returns {string | null}
 */
export function redirectFor(r, search) {
  switch (r.page) {
    case "start":
      return `/${search}`;
    case "home-legacy":
      return `/${search}`;
    case "today":
      return `/health${setParams(search, { block: "today" })}`;
    case "doctor":
      return `/health${setParams(search, { block: "doctor" })}`;
    case "checks":
      return `/health${setParams(search, { block: "checks" })}`;
    case "charts":
      return `/metrics${setParams(search, { tab: "system" })}`;
    case "traffic":
      return `/metrics${setParams(search, { tab: "traffic" })}`;
    case "timeline":
      return `/activity${setParams(search, { view: "timeline" })}`;
    default:
      return null;
  }
}

/**
 * @param {string} pathname a path, with or without a query string
 * @returns {Route}
 */
export function route(pathname) {
  const path = pathname.split(/[?#]/)[0];
  if (!path.startsWith("/")) return { page: "notfound", path };
  const rest = path.replace(/^\/+/, "").replace(/\/+$/, "");
  if (Object.hasOwn(PATH_TO_PAGE, rest)) {
    const page = PATH_TO_PAGE[rest];
    return /** @type {Route} */ ({ page });
  }
  const m = /^stacks\/([^/]+)(?:\/([^/]+))?$/.exec(rest);
  if (m) {
    const tab = STACK_TABS.find((t) => t.tab === (m[2] ?? "overview"));
    if (!tab) return { page: "notfound", path };
    try {
      return { page: "stack", name: decodeURIComponent(m[1]), tab: tab.tab };
    } catch {
      return { page: "notfound", path };
    }
  }
  return { page: "notfound", path };
}

/**
 * The link of one stack's page, or of one of its tabs.
 * @param {string} name
 * @param {StackTab} [tab]
 */
export const stackHref = (name, tab = "overview") =>
  `/stacks/${encodeURIComponent(name)}${tab === "overview" ? "" : `/${tab}`}`;

/**
 * The address a route answers at, the reverse of `route()`, for marking
 * the current entry in a nav built from the registry.
 * @param {Route} r
 * @returns {string | null}
 */
function hrefOf(r) {
  if (r.page === "stack") return stackHref(r.name, r.tab);
  if (r.page === "notfound") return null;
  const rest = Object.entries(PATH_TO_PAGE).find(([, p]) => p === r.page)?.[0];
  return rest == null ? null : `/${rest}`;
}

/**
 * @typedef {{href: string, label: string, current: boolean}} NavLink
 * @typedef {NavLink & {items?: NavLink[]}} NavEntry a link, or a group whose
 *   own link opens its first page and whose `items` fill its dropdown
 */

/**
 * The navigation bar for a route, built from the page registry (`nav:
 * true` pages, in the order and the groups the server sorted them into).
 * A page's `group` puts it in a dropdown of the bar; one without a group
 * is a top-level link. A stack page has no entry of its own in the
 * registry, so while one is open it gets its own entry beside the page it
 * was opened from.
 * @param {import("./pages.js").PageSet | null} pageSet
 * @param {Route} r
 * @returns {NavEntry[]}
 */
export function navEntries(pageSet, r) {
  /** @type {NavEntry[]} */
  const out = [];
  const current = hrefOf(r);
  for (const p of pageSet?.pages ?? []) {
    if (!p.nav) continue;
    const link = { href: p.path, label: p.title, current: p.path === current };
    if (!p.group) {
      out.push(link);
      continue;
    }
    const last = out[out.length - 1];
    if (last && last.items && last.label === p.group) {
      last.items.push(link);
      last.current = last.current || link.current;
    } else {
      out.push({
        href: p.path,
        label: p.group,
        current: link.current,
        items: [link],
      });
    }
  }
  if (r.page === "stack")
    out.splice(1, 0, {
      href: stackHref(r.name),
      label: `Stack ${r.name}`,
      current: true,
    });
  return out;
}

/**
 * The browser tab's title. Reads the registry when it has answered;
 * before that (first paint) it falls back to a local table, so the title
 * is never blank while `GET /api/kit/pages` is still in flight.
 * @param {Route} r
 * @param {import("./pages.js").PageSet | null} [pageSet]
 */
export function pageTitle(r, pageSet) {
  if (r.page === "stack") {
    const tab = STACK_TABS.find((t) => t.tab === r.tab);
    return r.tab === "overview"
      ? `Homelab · ${r.name}`
      : `Homelab · ${r.name} · ${tab?.label}`;
  }
  if (r.page === "notfound") return "Homelab · Not found";
  const fromRegistry = pageSet?.pages.find((p) => p.id === r.page)?.title;
  return `Homelab · ${fromRegistry ?? FALLBACK_TITLE[r.page] ?? r.page}`;
}
