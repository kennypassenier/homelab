// arch-frontend, feat-overview-8, feat-stacks-1: the router's pure half.
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

test("paths under /app/ name their page", () => {
  assert.deepEqual(route("/app/"), { page: "overview" });
  assert.deepEqual(route("/app"), { page: "overview" });
  assert.deepEqual(route("/app/home"), { page: "home" });
  assert.deepEqual(route("/app/health"), { page: "health" });
  assert.deepEqual(route("/app/metrics"), { page: "metrics" });
  assert.deepEqual(route("/app/host"), { page: "host" });
  assert.deepEqual(route("/app/activity"), { page: "activity" });
  assert.deepEqual(route("/app/log?source=media"), { page: "log" });
  assert.deepEqual(route("/app/shell"), { page: "shell" });
  assert.deepEqual(route("/app/apply"), { page: "apply" });
  assert.deepEqual(route("/app/presets"), { page: "presets" });
  assert.deepEqual(route("/app/stacks/media"), {
    page: "stack",
    name: "media",
    tab: "overview",
  });
  assert.equal(route("/app/nope").page, "notfound");
  assert.equal(route("/app/constructor").page, "notfound");
  assert.equal(route("/app/stacks/a/b").page, "notfound");
  assert.equal(route("/app/stacks/a/b/c").page, "notfound");
  assert.equal(route("/elsewhere").page, "notfound");
});

test("2026-09-30's retired paths still parse, for an old link or a Live view script", () => {
  assert.deepEqual(route("/app/start"), { page: "start" });
  assert.deepEqual(route("/app/today"), { page: "today" });
  assert.deepEqual(route("/app/doctor"), { page: "doctor" });
  assert.deepEqual(route("/app/checks/"), { page: "checks" });
  assert.deepEqual(route("/app/charts"), { page: "charts" });
  assert.deepEqual(route("/app/traffic"), { page: "traffic" });
  assert.deepEqual(route("/app/timeline?days=3"), { page: "timeline" });
});

test("redirectFor sends every retired path on to its merged page", () => {
  assert.equal(redirectFor(route("/app/start"), ""), "/app/home");
  assert.equal(redirectFor(route("/app/today"), ""), "/app/health?block=today");
  assert.equal(
    redirectFor(route("/app/doctor"), "?doctor.q=x"),
    "/app/health?doctor.q=x&block=doctor",
  );
  assert.equal(
    redirectFor(route("/app/checks"), ""),
    "/app/health?block=checks",
  );
  assert.equal(
    redirectFor(route("/app/charts"), "?stack=media&range=6h"),
    "/app/metrics?stack=media&range=6h&tab=system",
  );
  assert.equal(
    redirectFor(route("/app/traffic"), "?range=7d"),
    "/app/metrics?range=7d&tab=traffic",
  );
  assert.equal(
    redirectFor(route("/app/timeline"), "?days=30"),
    "/app/activity?days=30&view=timeline",
  );
  // Every other route: nothing to redirect.
  assert.equal(redirectFor(route("/app/"), ""), null);
  assert.equal(redirectFor(route("/app/health"), ""), null);
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
  assert.equal(stackHref("media"), "/app/stacks/media");
  assert.equal(stackHref("media", "logs"), "/app/stacks/media/logs");
  assert.deepEqual(route("/app/stacks/media/logs?app=sonarr"), {
    page: "stack",
    name: "media",
    tab: "logs",
  });
});

test("the navigation marks the current page, a stack page beside the overview", () => {
  const cur = (/** @type {string} */ p) =>
    navEntries(route(p))
      .filter((n) => n.current)
      .map((n) => n.label);
  assert.deepEqual(cur("/app/"), ["Overview"]);
  assert.deepEqual(cur("/app/host"), ["Host"]);
  assert.deepEqual(cur("/app/health"), ["Health"]);
  assert.deepEqual(cur("/app/metrics"), ["Metrics"]);
  // Kenny, 2026-09-30: Home, Overview, Health, Metrics, Activity first, each
  // of the last two a single page rather than a dropdown of them.
  const top = navEntries(route("/app/"));
  assert.deepEqual(
    top.map((n) => n.label),
    [
      "Home",
      "Overview",
      "Health",
      "Metrics",
      "Activity",
      "Host",
      "Operations",
      "Configure",
    ],
  );
  // Every page of the bar is reachable from it, in a group or on its own.
  const reached = top.flatMap((n) =>
    (n.items ? n.items : [n]).map((i) => i.href),
  );
  assert.equal(reached.length, 12);
  const onStack = navEntries(route("/app/stacks/media/apps"));
  assert.deepEqual(onStack[1], {
    href: "/app/stacks/media",
    label: "Stack media",
    current: true,
  });
  assert.deepEqual(cur("/app/nope"), []);
  assert.equal(pageTitle(route("/app/stacks/media")), "Homelab · media");
  assert.equal(
    pageTitle(route("/app/stacks/media/logs")),
    "Homelab · media · Logs",
  );
  assert.equal(pageTitle(route("/app/health")), "Homelab · Health");
  assert.equal(pageTitle(route("/app/x")), "Homelab · Not found");
  // Milestone act's pages; the notification centre is not in the bar.
  assert.deepEqual(route("/app/jobs?job=3"), { page: "jobs" });
  assert.deepEqual(route("/app/schedules"), { page: "schedules" });
  assert.deepEqual(route("/app/notifications"), { page: "notifications" });
  assert.equal(
    pageTitle(route("/app/notifications")),
    "Homelab · Notifications",
  );
  assert.deepEqual(cur("/app/notifications"), []);
  assert.deepEqual(cur("/app/jobs"), ["Operations"]);
  // Milestone edit's pages and tabs.
  assert.deepEqual(route("/app/firewall"), { page: "firewall" });
  assert.deepEqual(route("/app/settings"), { page: "settings" });
  assert.deepEqual(route("/app/stacks/kp-soft/firewall"), {
    page: "stack",
    name: "kp-soft",
    tab: "firewall",
  });
  assert.equal(
    pageTitle(route("/app/stacks/kp-soft/settings")),
    "Homelab · kp-soft · Settings",
  );
});
