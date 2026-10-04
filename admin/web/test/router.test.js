// arch-frontend, feat-overview-8, feat-stacks-1, nav-decisions, and
// feat-shell-1/3 (redesign 3.71.0, Kenny approved 2026-10-03): the router's
// pure half. Six areas `Apps · Inbox · Stacks · Activity │ Backups ·
// System`; every address a page ever had redirects to its new home in one
// hop. The nav bar renders from the page registry (pages.js), so these
// tests build a `PageSet` the way admin/src/main.rs registers it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { AREAS, areaOf, crumbs } from "../js/areas.js";
import {
  DRIVABLE_PATHS,
  PATH_TO_PAGE,
  RETIRED_STACK_TABS,
  STACK_TABS,
  navEntries,
  needsFleet,
  pageHref,
  pageTitle,
  redirectFor,
  route,
  shownPage,
  stackHref,
} from "../js/router.js";

/**
 * @param {string} id @param {string} title @param {string} path
 * @param {boolean} nav @param {string | null} [group]
 * @returns {import("../js/pages.js").RegPage}
 */
const reg = (id, title, path, nav, group = null) => ({
  id,
  title,
  path,
  group,
  order: 0,
  nav,
  source: "app",
  render: "app",
});

/** The registry as admin/src/main.rs registers it since 3.71.0. */
const pageSet = {
  app: "admin",
  brand: { title: "Homelab", href: "/" },
  home: "/",
  pages: [
    reg("home", "Apps", "/apps", true),
    reg("needs-you", "Inbox", "/needs-you", true),
    reg("overview", "Stacks", "/stacks", true),
    reg("activity", "Activity", "/activity", true),
    reg("backups", "Backups", "/backups", true),
    reg("system", "System", "/system", true),
    reg("host", "Host", "/host", false, "System"),
    reg("metrics", "Metrics", "/charts", false, "System"),
    reg("fleetview", "Map", "/map", false, "System"),
    reg("passkeys", "Passkeys", "/passkeys", false, "System"),
  ],
};

test("every current path lives at the root", () => {
  assert.deepEqual(route("/"), { page: "landing" });
  assert.deepEqual(route("/apps"), { page: "home" });
  assert.deepEqual(route("/needs-you"), { page: "needs-you" });
  assert.deepEqual(route("/stacks"), { page: "overview" });
  assert.deepEqual(route("/activity?view=planned"), { page: "activity" });
  assert.deepEqual(route("/backups"), { page: "backups" });
  assert.deepEqual(route("/system"), { page: "system" });
  assert.deepEqual(route("/charts"), { page: "metrics" });
  assert.deepEqual(route("/map"), { page: "fleetview" });
  assert.deepEqual(route("/system/notifications"), { page: "notifications" });
  assert.deepEqual(route("/console"), { page: "shell" });
  assert.deepEqual(route("/stacks/media"), {
    page: "stack",
    name: "media",
    tab: "overview",
  });
  assert.equal(route("/clients").page, "notfound");
  assert.equal(route("/nope").page, "notfound");
  assert.equal(route("/constructor").page, "notfound");
  assert.equal(route("/stacks/a/b").page, "notfound");
  assert.equal(route("/stacks/a/b/c").page, "notfound");
  assert.equal(route("elsewhere").page, "notfound");
});

test("feat-shell-1: every old address redirects to its new home in one hop, its query kept", () => {
  /** @type {[string, string][]} */
  const cases = [
    ["/overview", "/stacks"],
    ["/overview?fleet.q=media", "/stacks?fleet.q=media"],
    ["/overview?section=apply", "/stacks?deploy-all=1"],
    ["/apply", "/stacks?deploy-all=1"],
    ["/apply?fleet.q=media", "/stacks?fleet.q=media&deploy-all=1"],
    ["/health", "/needs-you"],
    ["/health?block=doctor", "/needs-you?kind=doctor"],
    ["/jobs?job=3", "/activity?job=3&view=running"],
    ["/log?source=media", "/activity?source=media&view=host-log"],
    ["/schedules", "/activity?view=planned"],
    ["/backupcalendar", "/backups?section=coverage"],
    ["/retired", "/backups?section=removed"],
    ["/fleetview?traffic=1", "/map?traffic=1"],
    ["/notifications", "/needs-you"],
    ["/shell?vmid=104", "/console?vmid=104"],
    ["/passkeys", "/settings?section=sign-in"],
    ["/secrets?stack=gateway", "/stacks/gateway/settings?section=secrets"],
    ["/status", "/needs-you"],
    ["/start", "/"],
    ["/today", "/needs-you?kind=today"],
    ["/doctor?doctor.q=x", "/needs-you?doctor.q=x&kind=doctor"],
    ["/checks/", "/needs-you?kind=checks"],
    ["/traffic?range=7d", "/charts?range=7d&tab=traffic"],
    ["/timeline?days=30", "/activity?days=30&view=timeline"],
    ["/home", "/apps"],
    ["/stacks/kp-soft/checks", "/stacks/kp-soft"],
    ["/stacks/kp-soft/firewall", "/stacks/kp-soft/settings?section=firewall"],
  ];
  for (const [from, to] of cases) {
    const [path, q = ""] = from.split("?");
    const got = redirectFor(route(path), q ? `?${q}` : "", {
      stacks: ["admin", "gateway"],
    });
    assert.equal(got, to, from);
    const landed = route(/** @type {string} */ (got));
    assert.notEqual(landed.page, "notfound", `${from} → ${got}`);
    assert.equal(
      redirectFor(landed, "", { stacks: ["admin"] }),
      null,
      `${from} → ${got} redirects again`,
    );
  }
  // /secrets without a stack: the fleet's first stack's Settings, once the
  // fleet is known; main.js waits for it (needsFleet).
  assert.equal(
    redirectFor(route("/secrets"), "", { stacks: ["admin", "gateway"] }),
    "/stacks/admin/settings?section=secrets",
  );
  assert.equal(needsFleet(route("/secrets"), ""), true);
  assert.equal(needsFleet(route("/secrets"), "?stack=x"), false);
  // A current route has nothing to redirect.
  for (const p of ["/", "/needs-you", "/stacks", "/stacks/media/logs"])
    assert.equal(redirectFor(route(p), ""), null, p);
});

test("feat-shell-1: every address the router knows is drivable, retired ones included", () => {
  for (const k of Object.keys(PATH_TO_PAGE)) {
    const r = route(`/${k}`);
    assert.notEqual(r.page, "notfound", k);
  }
  for (const k of ["overview", "apply", "health", "fleetview", "shell"])
    assert.ok(DRIVABLE_PATHS.includes(k), k);
  for (const k of [
    "",
    "apps",
    "needs-you",
    "stacks",
    "system",
    "map",
    "console",
  ])
    assert.ok(DRIVABLE_PATHS.includes(k), k);
});

test("every hub tab has its own path and round-trips; the merged tabs redirect", () => {
  assert.deepEqual(
    STACK_TABS.map((t) => t.tab),
    ["overview", "logs", "apps", "backups", "history", "settings"],
  );
  for (const { tab } of STACK_TABS) {
    const href = stackHref("odd name", tab);
    assert.deepEqual(route(href), { page: "stack", name: "odd name", tab });
  }
  for (const tab of RETIRED_STACK_TABS)
    assert.equal(route(`/stacks/media/${tab}`).page, "retired");
  assert.equal(stackHref("media"), "/stacks/media");
  assert.equal(stackHref("media", "logs"), "/stacks/media/logs");
});

test("feat-shell-1: a page module shown as a view of its new home is found there", () => {
  assert.equal(pageHref("schedules"), "/activity?view=planned");
  assert.equal(pageHref("fleetview"), "/map");
  assert.equal(pageHref("notifications"), "/system/notifications");
  assert.equal(pageHref("passkeys"), "/settings?section=sign-in");
  assert.equal(pageHref("nope"), null);
  assert.equal(shownPage("/activity", "?view=planned"), "schedules");
  assert.equal(shownPage("/activity", ""), "activity");
  // Senior review finding 15: Running now is part of Activity's own view,
  // so a Live view click on an activity-* control there stays put.
  assert.equal(shownPage("/activity", "?view=running"), "activity");
  assert.equal(shownPage("/backups", "?section=coverage"), "backupcalendar");
});

test("feat-shell-1: the bar is the six areas, the current one marked from any page inside it", () => {
  const top = navEntries(pageSet, route("/needs-you"), areaOf);
  assert.deepEqual(
    top.map((n) => n.label),
    ["Apps", "Inbox", "Stacks", "Activity", "Backups", "System"],
  );
  const cur = (/** @type {string} */ p) =>
    navEntries(pageSet, route(p), areaOf)
      .filter((n) => n.current)
      .map((n) => n.label);
  assert.deepEqual(cur("/apps"), ["Apps"]);
  assert.deepEqual(cur("/stacks/media/logs"), ["Stacks"]);
  assert.deepEqual(cur("/map"), ["System"]);
  assert.deepEqual(cur("/charts"), ["System"]);
  assert.deepEqual(cur("/activity"), ["Activity"]);
  assert.deepEqual(cur("/nope"), []);
  assert.equal(navEntries(null, route("/")).length, 0);
  assert.equal(pageTitle(route("/stacks/media")), "Homelab · media");
  assert.equal(
    pageTitle(route("/stacks/media/logs")),
    "Homelab · media · Logs",
  );
  assert.equal(pageTitle(route("/map"), pageSet), "Homelab · Map");
  // Before the registry has answered: the local fallback title, never blank.
  assert.equal(pageTitle(route("/needs-you")), "Homelab · Inbox");
  assert.equal(pageTitle(route("/x")), "Homelab · Not found");
});

test("feat-shell-1: the trail above a page names its area and links up", () => {
  const tab = (/** @type {string} */ t) =>
    STACK_TABS.find((x) => x.tab === t)?.label ?? t;
  assert.deepEqual(
    crumbs(route("/stacks/gateway/logs"), "/stacks/gateway/logs", "", {
      tabLabel: tab,
    }),
    [
      { label: "Stacks", href: "/stacks" },
      { label: "gateway", href: "/stacks/gateway" },
      { label: "Logs" },
    ],
  );
  assert.deepEqual(crumbs(route("/stacks/gateway"), "/stacks/gateway", ""), [
    { label: "Stacks", href: "/stacks" },
    { label: "gateway" },
  ]);
  assert.deepEqual(crumbs(route("/map"), "/map", ""), [
    { label: "System", href: "/system" },
    { label: "Map" },
  ]);
  assert.deepEqual(
    crumbs(route("/activity"), "/activity", "?view=planned&x=1"),
    [{ label: "Activity", href: "/activity" }, { label: "Planned" }],
  );
  assert.deepEqual(
    crumbs(route("/settings"), "/settings", "?section=sign-in"),
    [{ label: "System", href: "/system" }, { label: "Sign-in" }],
  );
  assert.deepEqual(crumbs(route("/needs-you"), "/needs-you", ""), [
    { label: "Inbox" },
  ]);
  assert.deepEqual(crumbs(route("/nope"), "/nope", ""), []);
});

test("feat-shell-1: the server registers exactly the six areas, in areas.js's order", () => {
  const src = readFileSync(
    new URL("../../src/main.rs", import.meta.url),
    "utf8",
  );
  const shown = [
    ...src.matchAll(
      /Page::new\("([^"]+)",\s*"([^"]+)",\s*"([^"]+)"\)(\s*\.[a-z_]+\([^)]*\))*/g,
    ),
  ]
    .filter((m) => !/\.hidden\(\)/.test(m[0]))
    .map((m) => ({ id: m[1], label: m[2], href: m[3] }));
  assert.deepEqual(
    shown,
    AREAS.map((a) => ({ id: a.id, label: a.label, href: a.href })),
  );
});
