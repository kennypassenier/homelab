import { setParams } from "./urlstate.js";

// The client-side router's pure half (arch-frontend): a path in, a page
// out. chassis answers index.html for every extensionless path the kit
// does not claim (`/api`, `/static`, `/login`, `/logout`, `/healthz`,
// `/readyz`, `/metrics` stay the kit's; `/passkeys` and `/status` are
// served by this app, fix-256). feat-overview-8: every page and every
// stack tab has its own path; what a page filters on lives in the query
// string (urlstate.js), so any state a person can see is a link they keep.
//
// feat-shell-1 (redesign 3.71.0, Kenny approved 2026-10-03): six areas,
// `Apps · Inbox · Stacks · Activity │ Backups · System` (areas.js holds
// the IA itself). Every address a page ever had keeps working: a retired
// path is `{page: "retired", from}` here and `redirectFor` names its new
// home, one hop, its query string kept. The nav bar and the command
// palette render from the page registry (`GET /api/kit/pages`, pages.js);
// this module keeps what the registry cannot know: which path is which SPA
// page, the stack hub's sub-router, the redirect table, and fallback
// titles for a page the registry has not answered yet.

/**
 * The stack hub's tabs (feat-stacks-1; feat-shell-3 reorders them by use,
 * FLOWS.md §1.3): Overview · Logs · Apps · Backups · History · Settings.
 */
export const STACK_TABS = /** @type {const} */ ([
  { tab: "overview", label: "Overview" },
  { tab: "logs", label: "Logs" },
  { tab: "apps", label: "Apps" },
  { tab: "backups", label: "Backups" },
  { tab: "history", label: "History" },
  { tab: "settings", label: "Settings" },
]);

/**
 * Tabs the hub had before 3.71.0, merged into another tab (FLOWS.md §2.1):
 * Checks into Overview ("Is it healthy?"), Firewall into Settings. Kept
 * parseable so an old link or a Live view `goto` still lands.
 */
export const RETIRED_STACK_TABS = /** @type {const} */ (["checks", "firewall"]);

/** @typedef {(typeof STACK_TABS)[number]["tab"]} StackTab */

/**
 * @typedef {{page: "landing"} | {page: "home"} | {page: "needs-you"} |
 *   {page: "overview"} | {page: "activity"} | {page: "backups"} |
 *   {page: "system"} | {page: "host"} | {page: "metrics"} |
 *   {page: "fleetview"} | {page: "firewall"} | {page: "settings"} |
 *   {page: "presets"} | {page: "notifications"} | {page: "shell"} |
 *   {page: "update"} | {page: "restore"} |
 *   {page: "stack", name: string, tab: StackTab} |
 *   {page: "retired", from: string, name?: string} |
 *   {page: "notfound", path: string}} Route
 * `landing` is `/`: the Inbox when it holds something, Apps otherwise
 * (Kenny, 2026-10-03). `retired` is any address a page used to live at;
 * `main.js` sends it on (`redirectFor`) before it mounts anything.
 */

/**
 * Every current address this app answers itself, by its route's `page`.
 * The page registry (pages.js) carries each one's title and area.
 * @type {Record<string, Exclude<Route["page"], "stack" | "retired" | "notfound">>}
 */
export const CURRENT_PATHS = {
  "": "landing",
  apps: "home",
  "needs-you": "needs-you",
  stacks: "overview",
  activity: "activity",
  backups: "backups",
  system: "system",
  host: "host",
  // fix-206: chassis reserves `/metrics` itself for its Prometheus scrape
  // text; the Metrics page lives at `/charts`.
  charts: "metrics",
  map: "fleetview",
  firewall: "firewall",
  settings: "settings",
  presets: "presets",
  "system/notifications": "notifications",
  console: "shell",
  // redesign-flows-11: the Update flow (`?all=1` or `?stack=…`).
  update: "update",
  // redesign-final-h3: the Restore flow, under Backups (FLOWS.md §5).
  "backups/restore": "restore",
};

/**
 * Every retired address and its new home (FLOWS.md §2.1 plus every older
 * alias): a function of the query string the old link carried. `ctx.stacks`
 * is the fleet's stack names, for `/secrets` (its first stack's Settings).
 * @type {Record<string, (search: string, ctx: {stacks: string[]}) => string>}
 */
const REDIRECTS = {
  // 3.71.0 IA (feat-shell-1)
  overview: (s) => {
    const p = new URLSearchParams(s);
    if (p.get("section") !== "apply") return `/stacks${s}`;
    return `/stacks${setParams(s, { section: null, "deploy-all": "1" })}`;
  },
  apply: (s) => `/stacks${setParams(s, { "deploy-all": "1" })}`,
  health: (s) => {
    const block = new URLSearchParams(s).get("block");
    return `/needs-you${setParams(s, { block: null, kind: block })}`;
  },
  jobs: (s) => `/activity${setParams(s, { view: "running" })}`,
  log: (s) => `/activity${setParams(s, { view: "host-log" })}`,
  schedules: (s) => `/activity${setParams(s, { view: "planned" })}`,
  backupcalendar: (s) => `/backups${setParams(s, { section: "coverage" })}`,
  retired: (s) => `/backups${setParams(s, { section: "removed" })}`,
  fleetview: (s) => `/map${s}`,
  notifications: (s) => `/needs-you${s}`,
  shell: (s) => `/console${s}`,
  passkeys: (s) => `/settings${setParams(s, { section: "sign-in" })}`,
  secrets: (s, ctx) => {
    const stack = new URLSearchParams(s).get("stack") ?? ctx.stacks[0];
    if (!stack) return `/stacks${setParams(s, { stack: null })}`;
    return `${stackHref(stack, "settings")}${setParams(s, { stack: null, section: "secrets" })}`;
  },
  // 2026-10-02: the kit's Status page is off; its address goes where
  // Health's did.
  status: (s) => `/needs-you${s}`,
  // 2026-09-30's retired addresses, collapsed to one hop.
  start: (s) => `/${s}`,
  today: (s) => `/needs-you${setParams(s, { kind: "today" })}`,
  doctor: (s) => `/needs-you${setParams(s, { kind: "doctor" })}`,
  checks: (s) => `/needs-you${setParams(s, { kind: "checks" })}`,
  traffic: (s) => `/charts${setParams(s, { tab: "traffic" })}`,
  timeline: (s) => `/activity${setParams(s, { view: "timeline" })}`,
  // Pre-3.1.0 address of the tile page.
  home: (s) => `/apps${s}`,
};

/**
 * Every address the router knows — current and retired — by what it is.
 * The whole-screen invariant "every address the router knows renders a
 * page or redirects to one" walks these keys.
 * @type {Record<string, string>}
 */
export const PATH_TO_PAGE = {
  ...CURRENT_PATHS,
  ...Object.fromEntries(Object.keys(REDIRECTS).map((k) => [k, "retired"])),
};

/**
 * The addresses a driven `goto` may land on (core::drive's `known_page`):
 * every address of `PATH_TO_PAGE`, retired ones included, so a Live view
 * script naming an old page still works (invariant 19). A web test holds
 * this equal to `formspec.json`'s `pages` (follow.test.js). drive-reach
 * (Kenny, 2026-10-03): every address the router knows, the 2026-09-30
 * aliases and the kit's old Status included; `goto /status` was refused
 * before the tab moved while a browser simply landed on the Inbox.
 */
export const DRIVABLE_PATHS = Object.keys(PATH_TO_PAGE);

/** drive-reach: the placeholder a redirect table entry names a stack by. */
export const STACK_SLOT = "{stack}";

/**
 * drive-reach: every retired address's new home as plain data, for the
 * dashboard's server (`drivecatalog.json`, built by
 * scripts/drivecatalog.mjs): the target `redirectFor` itself answers for
 * the address with no query string, a stack named by `STACK_SLOT` (the
 * merged stack tabs, and `/secrets`, whose target is the fleet's first
 * stack). Generated, never written by hand, so it cannot drift from the
 * redirect the browser takes.
 * @returns {Record<string, string>}
 */
export function redirectTable() {
  const slot = encodeURIComponent(STACK_SLOT);
  const fill = (/** @type {string} */ s) => s.split(slot).join(STACK_SLOT);
  /** @type {Record<string, string>} */
  const out = {};
  for (const k of Object.keys(REDIRECTS)) {
    const to = redirectFor(route(`/${k}`), "", { stacks: [STACK_SLOT] });
    if (to != null) out[k] = fill(to);
  }
  for (const tab of RETIRED_STACK_TABS) {
    const r = route(`/stacks/${slot}/${tab}`);
    const to = redirectFor(r, "");
    if (to != null) out[`stacks/${STACK_SLOT}/${tab}`] = fill(to);
  }
  return out;
}

/** Fallback titles for a page before the registry has answered. */
const FALLBACK_TITLE = /** @type {Record<string, string>} */ ({
  landing: "Homelab",
  home: "Apps",
  "needs-you": "Inbox",
  overview: "Stacks",
  activity: "Activity",
  backups: "Backups",
  system: "System",
  host: "Host",
  metrics: "Metrics",
  fleetview: "Map",
  firewall: "Firewall",
  settings: "Host settings",
  presets: "Presets",
  notifications: "Notification rules",
  shell: "Console",
  update: "Update apps",
  restore: "Restore",
});

/**
 * A pre-3.71.0 page's module, shown inside a current page until that page
 * draws it itself (a view of `?view=`/`?section=`/`?kind=`): the address a
 * Live view control declared on that module is found at (drivable.js
 * `page`), and the module main.js mounts for it.
 * @type {Record<string, {at: string, param: string, value: string}>}
 */
export const VIEWS = {
  // Running now is Activity's own view (senior review, finding 15): no
  // `jobs` entry, so `/activity?view=running` reports "activity" and a Live
  // view click on an activity-* control there keeps the filters.
  log: { at: "activity", param: "view", value: "host-log" },
  schedules: { at: "activity", param: "view", value: "planned" },
  backupcalendar: { at: "backups", param: "section", value: "coverage" },
  retired: { at: "backups", param: "section", value: "removed" },
  passkeys: { at: "settings", param: "section", value: "sign-in" },
  doctor: { at: "host", param: "section", value: "doctor" },
};

/**
 * The retired path's replacement, its query string kept and extended so
 * the merged page opens on the right view; `null` for a current route.
 * @param {Route} r
 * @param {string} search e.g. location.search
 * @param {{stacks?: string[]}} [ctx] the fleet's stack names, when known
 * @returns {string | null}
 */
export function redirectFor(r, search, ctx = {}) {
  const c = { stacks: ctx.stacks ?? [] };
  if (r.page === "retired") {
    if (r.name != null) {
      // A merged stack tab (FLOWS.md §1.3).
      if (r.from === "checks") return `${stackHref(r.name)}${search}`;
      return `${stackHref(r.name, "settings")}${setParams(search, { section: r.from })}`;
    }
    const f = REDIRECTS[r.from];
    return f ? f(search, c) : null;
  }
  return null;
}

/**
 * Whether a retired route needs the fleet's stack list to know its new
 * home (`/secrets` without `?stack=`); main.js waits for the first fleet
 * read before redirecting it.
 * @param {Route} r
 * @param {string} search
 */
export const needsFleet = (r, search) =>
  r.page === "retired" &&
  r.from === "secrets" &&
  !new URLSearchParams(search).has("stack");

/**
 * @param {string} pathname a path, with or without a query string
 * @returns {Route}
 */
export function route(pathname) {
  const path = pathname.split(/[?#]/)[0];
  if (!path.startsWith("/")) return { page: "notfound", path };
  const rest = path.replace(/^\/+/, "").replace(/\/+$/, "");
  if (Object.hasOwn(CURRENT_PATHS, rest))
    return /** @type {Route} */ ({ page: CURRENT_PATHS[rest] });
  if (Object.hasOwn(REDIRECTS, rest)) return { page: "retired", from: rest };
  const m = /^stacks\/([^/]+)(?:\/([^/]+))?$/.exec(rest);
  if (m) {
    /** @type {string} */
    const want = m[2] ?? "overview";
    let name;
    try {
      name = decodeURIComponent(m[1]);
    } catch {
      return { page: "notfound", path };
    }
    if (/** @type {readonly string[]} */ (RETIRED_STACK_TABS).includes(want))
      return { page: "retired", from: want, name };
    const tab = STACK_TABS.find((t) => t.tab === want);
    if (!tab) return { page: "notfound", path };
    return { page: "stack", name, tab: tab.tab };
  }
  return { page: "notfound", path };
}

/**
 * The page module a location shows: the route's page, or a pre-3.71.0
 * module drawn as one of its views (`VIEWS`). pagedrive.js compares it with
 * a declared control's page.
 * @param {string} pathname
 * @param {string} search
 * @returns {string}
 */
export function shownPage(pathname, search) {
  const r = route(pathname);
  const p = new URLSearchParams(search);
  for (const [page, v] of Object.entries(VIEWS))
    if (v.at === r.page && p.get(v.param) === v.value) return page;
  return r.page;
}

/**
 * The link of one stack's hub, or of one of its tabs.
 * @param {string} name
 * @param {StackTab | string} [tab]
 */
export const stackHref = (name, tab = "overview") =>
  `/stacks/${encodeURIComponent(name)}${tab === "overview" ? "" : `/${tab}`}`;

/**
 * The address of a page by its id (`"fleetview"` → `/map`), a pre-3.71.0
 * module shown as a view included (`"schedules"` → `/activity?view=planned`),
 * so a Live view control declared on a page is found wherever the page
 * lives now (fix-239); null for an id no route answers.
 * @param {string} page
 * @returns {string | null}
 */
export function pageHref(page) {
  const view = VIEWS[page];
  if (view) {
    const at = pageHref(view.at);
    return at == null ? null : `${at}?${view.param}=${view.value}`;
  }
  const rest = Object.entries(CURRENT_PATHS).find(([, p]) => p === page)?.[0];
  return rest == null ? null : `/${rest}`;
}

/**
 * @typedef {{href: string, label: string, current: boolean}} NavLink
 * @typedef {NavLink & {items?: NavLink[]}} NavEntry a link, or a group whose
 *   own link opens its first page and whose `items` fill its dropdown
 */

/**
 * The navigation bar for a route, built from the page registry (`nav:
 * true` pages, in the order the server registered them). A page's area
 * (areas.js) is current while any of its sub-pages or stack hubs is open.
 * @param {import("./pages.js").PageSet | null} pageSet
 * @param {Route} r
 * @param {(r: Route) => string | null} [areaOf] the area id of a route
 * @returns {NavEntry[]}
 */
export function navEntries(pageSet, r, areaOf) {
  /** @type {NavEntry[]} */
  const out = [];
  const area = areaOf ? areaOf(r) : null;
  for (const p of pageSet?.pages ?? []) {
    if (!p.nav) continue;
    const current = area != null ? p.id === area : p.id === r.page;
    const link = { href: p.path, label: p.title, current };
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
  if (r.page === "landing" || r.page === "retired") return "Homelab";
  const fromRegistry = pageSet?.pages.find((p) => p.id === r.page)?.title;
  return `Homelab · ${fromRegistry ?? FALLBACK_TITLE[r.page] ?? r.page}`;
}
