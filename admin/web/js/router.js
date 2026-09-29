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
 *   {page: "today"} | {page: "log"} | {page: "shell"} | {page: "apply"} |
 *   {page: "presets"} | {page: "notfound", path: string}} Route
 */

/**
 * The navigation bar, in order. `group` puts a page in a dropdown of the bar
 * (Kenny, 2026-09-29: thirteen top-level links wrapped onto a second row at
 * 1280 px); a page without one is a top-level link.
 */
export const NAV = /** @type {const} */ ([
  { page: "overview", href: "/app/", label: "Overview" },
  // TUI parity: `homelab today`, the morning question in one answer.
  { page: "today", href: "/app/today", label: "Today" },
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
    page: "activity",
    href: "/app/activity",
    label: "Activity",
    group: "History",
  },
  {
    page: "timeline",
    href: "/app/timeline",
    label: "Timeline",
    group: "History",
  },
  { page: "checks", href: "/app/checks", label: "Checks", group: "Health" },
  { page: "doctor", href: "/app/doctor", label: "Doctor", group: "Health" },
  {
    page: "firewall",
    href: "/app/firewall",
    label: "Firewall",
    group: "Configure",
  },
  {
    page: "settings",
    href: "/app/settings",
    label: "Settings",
    group: "Configure",
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
  today: { page: "today" },
  log: { page: "log" },
  shell: { page: "shell" },
  apply: { page: "apply" },
  presets: { page: "presets" },
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
  return `Homelab · ${n?.label}`;
}
