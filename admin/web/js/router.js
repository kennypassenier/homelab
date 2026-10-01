import { setParams } from "./urlstate.js";

// The client-side router's pure half (arch-frontend): a path in, a page
// out. chassis answers index.html for every extensionless path under /app/.
// feat-overview-8: every page and every stack tab has its own path; what a
// page filters on lives in the query string (urlstate.js), so any state a
// person can see is a link they can keep.

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
 *   {page: "backups"} | {page: "secrets"} |
 *   {page: "log"} | {page: "shell"} | {page: "apply"} |
 *   {page: "presets"} |
 *   {page: "fleetview"} | {page: "backupcalendar"} |
 *   {page: "start"} | {page: "today"} | {page: "doctor"} | {page: "checks"} |
 *   {page: "charts"} | {page: "traffic"} | {page: "timeline"} |
 *   {page: "notfound", path: string}} Route
 * The last group (start, today, doctor, checks, charts, traffic, timeline)
 * are 2026-09-30's retired addresses: `route()` still names them, so an old
 * link or a Live view script naming one is recognised, but `main.js`
 * redirects every one of them before it ever mounts a page for them (see
 * `redirectFor`).
 */

/**
 * The navigation bar, in order. `group` puts a page in a dropdown of the bar
 * (Kenny, 2026-09-29: thirteen top-level links wrapped onto a second row at
 * 1280 px); a page without one is a top-level link. 2026-09-30: Home,
 * Overview, Health, Metrics, Activity first (Kenny's order), each of the
 * last three now one page instead of a dropdown of them.
 */
export const NAV = /** @type {const} */ ([
  // replace-homepage: the tiles the stacks declare, renamed from Start.
  { page: "home", href: "/app/home", label: "Home" },
  { page: "overview", href: "/app/", label: "Overview" },
  // feat-health-1: Today, Doctor and Checks, merged.
  { page: "health", href: "/app/health", label: "Health" },
  // feat-metrics-1 (replace-grafana + replace-goaccess): Charts and
  // Traffic, merged.
  { page: "metrics", href: "/app/metrics", label: "Metrics" },
  // feat-activity-1: Activity and Timeline, merged.
  { page: "activity", href: "/app/activity", label: "Activity" },
  { page: "host", href: "/app/host", label: "Host" },
  // TUI parity: LOG_STREAM, every host operation whoever started it.
  { page: "log", href: "/app/log", label: "Live log", group: "Operations" },
  { page: "jobs", href: "/app/jobs", label: "Jobs", group: "Operations" },
  // TUI parity: `homelab apply`, plan first.
  { page: "apply", href: "/app/apply", label: "Apply", group: "Operations" },
  {
    page: "schedules",
    href: "/app/schedules",
    label: "Schedules",
    group: "Operations",
  },
  {
    page: "firewall",
    href: "/app/firewall",
    label: "Firewall",
    group: "Configure",
  },
  {
    page: "backups",
    href: "/app/backups",
    label: "Backups",
    group: "Configure",
  },
  {
    page: "secrets",
    href: "/app/secrets",
    label: "Secrets",
    group: "Configure",
  },
  {
    page: "settings",
    href: "/app/settings",
    label: "Settings",
    group: "Configure",
  },
  // visuals (feat-overview-7/10/11/12, feat-stacks-9/10): the fleet-wide
  // graphs, grouped apart from the single-stack and single-purpose pages
  // above so Configure does not grow a sixth unrelated entry.
  {
    page: "fleetview",
    href: "/app/fleetview",
    label: "Fleet view",
    group: "Visuals",
  },
  {
    page: "backupcalendar",
    href: "/app/backupcalendar",
    label: "Backup calendar",
    group: "Visuals",
  },
]);

/** Pages with a fixed address that the bar does not list (the bell opens it). */
export const OTHER_PAGES = /** @type {const} */ ([
  {
    page: "notifications",
    href: "/app/notifications",
    label: "Notifications",
  },
  // TUI parity: the SHELL tab (the host page and a stack page link it) and
  // the preset catalogue (`homelab presets`; the new-stack wizard links it).
  { page: "shell", href: "/app/shell", label: "Shell" },
  { page: "presets", href: "/app/presets", label: "Presets" },
]);

/** @type {Record<string, Route>} */
const FIXED = {
  "": { page: "overview" },
  home: { page: "home" },
  health: { page: "health" },
  metrics: { page: "metrics" },
  host: { page: "host" },
  activity: { page: "activity" },
  jobs: { page: "jobs" },
  schedules: { page: "schedules" },
  notifications: { page: "notifications" },
  firewall: { page: "firewall" },
  backups: { page: "backups" },
  secrets: { page: "secrets" },
  settings: { page: "settings" },
  log: { page: "log" },
  shell: { page: "shell" },
  apply: { page: "apply" },
  presets: { page: "presets" },
  fleetview: { page: "fleetview" },
  backupcalendar: { page: "backupcalendar" },
  // 2026-09-30: retired addresses, kept parseable for old links and for
  // Live view's known_page (core::drive checks formspec.json's page list,
  // not this Route); `redirectFor` sends every one of them on to its new
  // home before main.js ever mounts a page for them.
  start: { page: "start" },
  today: { page: "today" },
  doctor: { page: "doctor" },
  checks: { page: "checks" },
  charts: { page: "charts" },
  traffic: { page: "traffic" },
  timeline: { page: "timeline" },
};

/**
 * The retired path's replacement, its query string kept and extended so
 * the merged page opens on the right block or tab; `null` for a path that
 * is not retired.
 * @param {Route} r
 * @param {string} search e.g. location.search
 * @returns {string | null}
 */
export function redirectFor(r, search) {
  switch (r.page) {
    case "start":
      return `/app/home${search}`;
    case "today":
      return `/app/health${setParams(search, { block: "today" })}`;
    case "doctor":
      return `/app/health${setParams(search, { block: "doctor" })}`;
    case "checks":
      return `/app/health${setParams(search, { block: "checks" })}`;
    case "charts":
      return `/app/metrics${setParams(search, { tab: "system" })}`;
    case "traffic":
      return `/app/metrics${setParams(search, { tab: "traffic" })}`;
    case "timeline":
      return `/app/activity${setParams(search, { view: "timeline" })}`;
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
  if (path !== "/app" && !path.startsWith("/app/"))
    return { page: "notfound", path };
  const rest = path.replace(/^\/app\/?/, "").replace(/\/+$/, "");
  if (Object.hasOwn(FIXED, rest)) return FIXED[rest];
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
  `/app/stacks/${encodeURIComponent(name)}${tab === "overview" ? "" : `/${tab}`}`;

/**
 * @typedef {{href: string, label: string, current: boolean}} NavLink
 * @typedef {NavLink & {items?: NavLink[]}} NavEntry a link, or a group whose
 *   own link opens its first page and whose `items` fill its dropdown
 */

/**
 * The navigation bar for a route, the current page marked (a group is
 * current when one of its pages is). A stack page has no fixed link, so
 * while one is open it gets its own entry beside the overview it was opened
 * from.
 * @param {Route} r
 * @returns {NavEntry[]}
 */
export function navEntries(r) {
  /** @type {NavEntry[]} */
  const out = [];
  for (const n of NAV) {
    const link = { href: n.href, label: n.label, current: n.page === r.page };
    const group = "group" in n ? n.group : undefined;
    if (!group) {
      out.push(link);
      continue;
    }
    const last = out[out.length - 1];
    if (last && last.items && last.label === group) {
      last.items.push(link);
      last.current = last.current || link.current;
    } else {
      out.push({
        href: n.href,
        label: group,
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
 * The browser tab's title.
 * @param {Route} r
 */
export function pageTitle(r) {
  if (r.page === "stack") {
    const tab = STACK_TABS.find((t) => t.tab === r.tab);
    return r.tab === "overview"
      ? `Homelab · ${r.name}`
      : `Homelab · ${r.name} · ${tab?.label}`;
  }
  if (r.page === "notfound") return "Homelab · Not found";
  const n = [...NAV, ...OTHER_PAGES].find((x) => x.page === r.page);
  return `Homelab · ${n?.label ?? r.page}`;
}
