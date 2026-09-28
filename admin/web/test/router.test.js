// arch-frontend: the router's pure half.
import { test } from "node:test";
import assert from "node:assert/strict";
import { navEntries, pageTitle, route, stackHref } from "../js/router.js";

test("paths under /app/ name their page", () => {
  assert.deepEqual(route("/app/"), { page: "overview" });
  assert.deepEqual(route("/app"), { page: "overview" });
  assert.deepEqual(route("/app/activity"), { page: "activity" });
  assert.deepEqual(route("/app/checks/"), { page: "checks" });
  assert.deepEqual(route("/app/doctor"), { page: "doctor" });
  assert.deepEqual(route("/app/stacks/media"), {
    page: "stack",
    name: "media",
  });
  assert.equal(route("/app/nope").page, "notfound");
  assert.equal(route("/app/stacks/a/b").page, "notfound");
  assert.equal(route("/elsewhere").page, "notfound");
});

test("a stack link round-trips through the router", () => {
  const name = "odd name";
  assert.deepEqual(route(stackHref(name)), { page: "stack", name });
});

test("the navigation marks the current page, a stack page beside the overview", () => {
  const cur = (/** @type {string} */ p) =>
    navEntries(route(p))
      .filter((n) => n.current)
      .map((n) => n.label);
  assert.deepEqual(cur("/app/"), ["Overview"]);
  assert.deepEqual(cur("/app/doctor"), ["Doctor"]);
  const onStack = navEntries(route("/app/stacks/media"));
  assert.equal(onStack.length, 5);
  assert.deepEqual(onStack[1], {
    href: "/app/stacks/media",
    label: "Stack media",
    current: true,
  });
  assert.deepEqual(cur("/app/nope"), []);
  assert.equal(pageTitle(route("/app/stacks/media")), "Homelab · media");
});
