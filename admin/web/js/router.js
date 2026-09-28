// The client-side router's pure half (arch-frontend): a path in, a page
// out. chassis answers index.html for every extensionless path under /app/.

/**
 * @typedef {{page: "overview"} | {page: "stack", name: string} |
 *   {page: "activity"} | {page: "checks"} | {page: "doctor"} |
 *   {page: "notfound", path: string}} Route
 */

/** The navigation bar, in order. */
export const NAV = /** @type {const} */ ([
  { page: "overview", href: "/app/", label: "Overview" },
  { page: "activity", href: "/app/activity", label: "Activity" },
  { page: "checks", href: "/app/checks", label: "Checks" },
  { page: "doctor", href: "/app/doctor", label: "Doctor" },
]);

/**
 * @param {string} pathname
 * @returns {Route}
 */
export function route(pathname) {
  const rest = pathname.replace(/^\/app\/?/, "").replace(/\/+$/, "");
  if (pathname !== "/app" && !pathname.startsWith("/app/"))
    return { page: "notfound", path: pathname };
  if (rest === "") return { page: "overview" };
  if (rest === "activity") return { page: "activity" };
  if (rest === "checks") return { page: "checks" };
  if (rest === "doctor") return { page: "doctor" };
  const m = /^stacks\/([^/]+)$/.exec(rest);
  if (m) {
    try {
      return { page: "stack", name: decodeURIComponent(m[1]) };
    } catch {
      return { page: "notfound", path: pathname };
    }
  }
  return { page: "notfound", path: pathname };
}

/**
 * The link of one stack's page.
 * @param {string} name
 */
export const stackHref = (name) => `/app/stacks/${encodeURIComponent(name)}`;

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
  switch (r.page) {
    case "overview":
      return "Homelab · Overview";
    case "stack":
      return `Homelab · ${r.name}`;
    case "activity":
      return "Homelab · Activity";
    case "checks":
      return "Homelab · Checks";
    case "doctor":
      return "Homelab · Doctor";
    default:
      return "Homelab · Not found";
  }
}
