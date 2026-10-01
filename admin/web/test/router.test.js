// arch-frontend, feat-overview-8, feat-stacks-1, nav-decisions: the
// router's pure half. The nav bar itself renders from the page registry
// (pages.js, `GET /api/kit/pages`) rather than a list in this module, so
// these tests build a small sample `PageSet` the way the server would.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  STACK_TABS,
  navEntries,
  pageTitle,
  redirectFor,
  route,
  stackHref,
} from "../js/router.js";

/**
 * A page registry the way `GET /api/kit/pages` answers it.
 * @type {import("../js/pages.js").PageSet}
 */
const pageSet = {
  app: "admin",
  brand: { title: "Homelab", href: "/overview" },
  home: "/",
  pages: [
    {
      id: "home",
      title: "Apps",
      path: "/",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "overview",
      title: "Overview",
      path: "/overview",
      group: null,
      order: 0,
      nav: false,
      source: "app",
      render: "app",
    },
    {
      id: "health",
      title: "Health",
      path: "/health",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "metrics",
      title: "Metrics",
      path: "/metrics",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "activity",
      title: "Activity",
      path: "/activity",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "host",
      title: "Host",
      path: "/host",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "log",
      title: "Live log",
      path: "/log",
      group: "Operations",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "jobs",
      title: "Jobs",
      path: "/jobs",
      group: "Operations",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "apply",
      title: "Apply",
      path: "/apply",
      group: "Operations",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "schedules",
      title: "Schedules",
      path: "/schedules",
      group: "Operations",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "firewall",
      title: "Firewall",
      path: "/firewall",
      group: "Configure",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "backups",
      title: "Backups",
      path: "/backups",
      group: "Configure",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "secrets",
      title: "Secrets",
      path: "/secrets",
      group: "Configure",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "settings",
      title: "Settings",
      path: "/settings",
      group: "Configure",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "fleetview",
      title: "Fleet view",
      path: "/fleetview",
      group: "Visuals",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "backupcalendar",
      title: "Backup calendar",
      path: "/backupcalendar",
      group: "Visuals",
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "notifications",
      title: "Notifications",
      path: "/notifications",
      group: null,
      order: 0,
      nav: false,
      source: "app",
      render: "app",
    },
    {
      id: "shell",
      title: "Shell",
      path: "/shell",
      group: null,
      order: 0,
      nav: false,
      source: "app",
      render: "app",
    },
    {
      id: "presets",
      title: "Presets",
      path: "/presets",
      group: null,
      order: 0,
      nav: false,
      source: "app",
      render: "app",
    },
    {
      id: "status",
      title: "Status",
      path: "/status",
      group: null,
      order: 1000,
      nav: true,
      source: "kit",
      render: "app",
    },
    {
      id: "clients",
      title: "Clients",
      path: "/clients",
      group: null,
      order: 1010,
      nav: true,
      source: "kit",
      render: "app",
    },
    {
      id: "passkeys",
      title: "Passkeys",
      path: "/passkeys",
      group: null,
      order: 1020,
      nav: true,
      source: "kit",
      render: "app",
    },
  ],
};

test("every path lives at the root", () => {
  assert.deepEqual(route("/"), { page: "home" });
  assert.deepEqual(route("/overview"), { page: "overview" });
  assert.deepEqual(route("/health"), { page: "health" });
  assert.deepEqual(route("/metrics"), { page: "metrics" });
  assert.deepEqual(route("/host"), { page: "host" });
  assert.deepEqual(route("/activity"), { page: "activity" });
  assert.deepEqual(route("/log?source=media"), { page: "log" });
  assert.deepEqual(route("/shell"), { page: "shell" });
  assert.deepEqual(route("/apply"), { page: "apply" });
  assert.deepEqual(route("/presets"), { page: "presets" });
  assert.deepEqual(route("/status"), { page: "status" });
  assert.deepEqual(route("/clients"), { page: "clients" });
  assert.deepEqual(route("/passkeys"), { page: "passkeys" });
  assert.deepEqual(route("/stacks/media"), {
    page: "stack",
    name: "media",
    tab: "overview",
  });
  assert.equal(route("/nope").page, "notfound");
  assert.equal(route("/constructor").page, "notfound");
  assert.equal(route("/stacks/a/b").page, "notfound");
  assert.equal(route("/stacks/a/b/c").page, "notfound");
  assert.equal(route("elsewhere").page, "notfound");
});

test("retired and pre-3.1.0 paths still parse, for an old link or a Live view script", () => {
  assert.deepEqual(route("/start"), { page: "start" });
  assert.deepEqual(route("/today"), { page: "today" });
  assert.deepEqual(route("/doctor"), { page: "doctor" });
  assert.deepEqual(route("/checks/"), { page: "checks" });
  assert.deepEqual(route("/charts"), { page: "charts" });
  assert.deepEqual(route("/traffic"), { page: "traffic" });
  assert.deepEqual(route("/timeline?days=3"), { page: "timeline" });
  assert.deepEqual(route("/home"), { page: "home-legacy" });
});

test("redirectFor sends every retired or pre-3.1.0 path on to its home", () => {
  assert.equal(redirectFor(route("/start"), ""), "/");
  assert.equal(redirectFor(route("/home"), ""), "/");
  assert.equal(redirectFor(route("/today"), ""), "/health?block=today");
  assert.equal(
    redirectFor(route("/doctor"), "?doctor.q=x"),
    "/health?doctor.q=x&block=doctor",
  );
  assert.equal(redirectFor(route("/checks"), ""), "/health?block=checks");
  assert.equal(
    redirectFor(route("/charts"), "?stack=media&range=6h"),
    "/metrics?stack=media&range=6h&tab=system",
  );
  assert.equal(
    redirectFor(route("/traffic"), "?range=7d"),
    "/metrics?range=7d&tab=traffic",
  );
  assert.equal(
    redirectFor(route("/timeline"), "?days=30"),
    "/activity?days=30&view=timeline",
  );
  // Every other route: nothing to redirect.
  assert.equal(redirectFor(route("/"), ""), null);
  assert.equal(redirectFor(route("/health"), ""), null);
  assert.equal(
    redirectFor({ page: "stack", name: "media", tab: "overview" }, ""),
    null,
  );
});

test("every stack tab has its own path and round-trips", () => {
  for (const { tab } of STACK_TABS) {
    const href = stackHref("odd name", tab);
    assert.deepEqual(route(href), { page: "stack", name: "odd name", tab });
  }
  assert.equal(stackHref("media"), "/stacks/media");
  assert.equal(stackHref("media", "logs"), "/stacks/media/logs");
  assert.deepEqual(route("/stacks/media/logs?app=sonarr"), {
    page: "stack",
    name: "media",
    tab: "logs",
  });
});

test("the navigation renders from the registry and marks the current page", () => {
  const cur = (/** @type {string} */ p) =>
    navEntries(pageSet, route(p))
      .filter((n) => n.current)
      .map((n) => n.label);
  assert.deepEqual(cur("/"), ["Apps"]);
  assert.deepEqual(cur("/host"), ["Host"]);
  assert.deepEqual(cur("/health"), ["Health"]);
  assert.deepEqual(cur("/metrics"), ["Metrics"]);
  // Overview is hidden from the bar (the brand link opens it instead).
  assert.deepEqual(cur("/overview"), []);
  const top = navEntries(pageSet, route("/"));
  assert.deepEqual(
    top.map((n) => n.label),
    [
      "Apps",
      "Health",
      "Metrics",
      "Activity",
      "Host",
      "Operations",
      "Configure",
      "Visuals",
      "Status",
      "Clients",
      "Passkeys",
    ],
  );
  // Every page of the bar is reachable from it, in a group or on its own.
  const reached = top.flatMap((n) =>
    (n.items ? n.items : [n]).map((i) => i.href),
  );
  assert.equal(reached.length, 16);
  assert.equal(navEntries(null, route("/")).length, 0);
  const onStack = navEntries(pageSet, route("/stacks/media/apps"));
  assert.deepEqual(onStack[1], {
    href: "/stacks/media",
    label: "Stack media",
    current: true,
  });
  assert.deepEqual(cur("/nope"), []);
  assert.equal(pageTitle(route("/stacks/media")), "Homelab · media");
  assert.equal(
    pageTitle(route("/stacks/media/logs")),
    "Homelab · media · Logs",
  );
  assert.equal(pageTitle(route("/health"), pageSet), "Homelab · Health");
  // Before the registry has answered: the local fallback title, never blank.
  assert.equal(pageTitle(route("/health")), "Homelab · Health");
  assert.equal(pageTitle(route("/x")), "Homelab · Not found");
  assert.deepEqual(route("/jobs?job=3"), { page: "jobs" });
  assert.deepEqual(route("/schedules"), { page: "schedules" });
  assert.deepEqual(route("/notifications"), { page: "notifications" });
  assert.equal(
    pageTitle(route("/notifications"), pageSet),
    "Homelab · Notifications",
  );
  assert.deepEqual(cur("/notifications"), []);
  assert.deepEqual(cur("/jobs"), ["Operations"]);
  assert.deepEqual(route("/firewall"), { page: "firewall" });
  assert.deepEqual(route("/settings"), { page: "settings" });
  assert.deepEqual(route("/stacks/kp-soft/firewall"), {
    page: "stack",
    name: "kp-soft",
    tab: "firewall",
  });
  assert.equal(
    pageTitle(route("/stacks/kp-soft/settings")),
    "Homelab · kp-soft · Settings",
  );
});
