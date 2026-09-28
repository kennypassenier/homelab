// arch-frontend, feat-overview-8, feat-stacks-1: the router's pure half.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  STACK_TABS,
  navEntries,
  pageTitle,
  route,
  stackHref,
} from "../js/router.js";

test("paths under /app/ name their page", () => {
  assert.deepEqual(route("/app/"), { page: "overview" });
  assert.deepEqual(route("/app"), { page: "overview" });
  assert.deepEqual(route("/app/host"), { page: "host" });
  assert.deepEqual(route("/app/activity"), { page: "activity" });
  assert.deepEqual(route("/app/timeline?days=3"), { page: "timeline" });
  assert.deepEqual(route("/app/checks/"), { page: "checks" });
  assert.deepEqual(route("/app/doctor"), { page: "doctor" });
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
  assert.deepEqual(cur("/app/doctor"), ["Doctor"]);
  assert.deepEqual(cur("/app/host"), ["Host"]);
  const onStack = navEntries(route("/app/stacks/media/apps"));
  // Eight pages, milestone edit's Firewall and Settings, and the stack.
  assert.equal(onStack.length, 11);
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
  assert.equal(pageTitle(route("/app/timeline")), "Homelab · Timeline");
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
  assert.deepEqual(cur("/app/jobs"), ["Jobs"]);
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
