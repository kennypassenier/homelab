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
 * @typedef {{page: "overview"} | {page: "host"} |
 *   {page: "stack", name: string, tab: StackTab} |
 *   {page: "activity"} | {page: "timeline"} | {page: "checks"} |
 *   {page: "doctor"} | {page: "jobs"} | {page: "schedules"} |
 *   {page: "notifications"} | {page: "firewall"} | {page: "settings"} |
 *   {page: "notfound", path: string}} Route
 */

/** The navigation bar, in order. */
export const NAV = /** @type {const} */ ([
  { page: "overview", href: "/app/", label: "Overview" },
  { page: "host", href: "/app/host", label: "Host" },
  { page: "activity", href: "/app/activity", label: "Activity" },
  { page: "timeline", href: "/app/timeline", label: "Timeline" },
  { page: "checks", href: "/app/checks", label: "Checks" },
  { page: "doctor", href: "/app/doctor", label: "Doctor" },
  { page: "jobs", href: "/app/jobs", label: "Jobs" },
  { page: "schedules", href: "/app/schedules", label: "Schedules" },
  { page: "firewall", href: "/app/firewall", label: "Firewall" },
  { page: "settings", href: "/app/settings", label: "Settings" },
]);

/** Pages with a fixed address that the bar does not list (the bell opens it). */
export const OTHER_PAGES = /** @type {const} */ ([
  {
    page: "notifications",
    href: "/app/notifications",
    label: "Notifications",
  },
]);

/** @type {Record<string, Route>} */
const FIXED = {
  "": { page: "overview" },
  host: { page: "host" },
  activity: { page: "activity" },
  timeline: { page: "timeline" },
  checks: { page: "checks" },
  doctor: { page: "doctor" },
  jobs: { page: "jobs" },
  schedules: { page: "schedules" },
  notifications: { page: "notifications" },
  firewall: { page: "firewall" },
  settings: { page: "settings" },
};

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
 * The navigation bar for a route, the current page marked. A stack page has
 * no fixed link, so while one is open it gets its own entry beside the
 * overview it was opened from.
 * @param {Route} r
 * @returns {{href: string, label: string, current: boolean}[]}
 */
export function navEntries(r) {
  /** @type {{href: string, label: string, current: boolean}[]} */
  const out = NAV.map((n) => ({
    href: n.href,
    label: n.label,
    current: n.page === r.page,
  }));
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
  return `Homelab · ${n?.label}`;
}
