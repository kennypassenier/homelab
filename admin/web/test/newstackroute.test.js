// ux-stacknew-2: New stack's wizard runs on a page of its own, one address per
// route, and a stack called "new" still reaches its own hub.
import assert from "node:assert/strict";
import test from "node:test";
import { areaOf, crumbs } from "../js/areas.js";
import { route } from "../js/router.js";
import { kindOf } from "../js/pages/newstack.js";

test("ux-stacknew-2: the three New stack routes are pages of the Stacks area", () => {
  for (const kind of ["preset", "bundle", "empty"]) {
    const r = route(`/stacks/new/${kind}`);
    assert.deepEqual(r, { page: "newstack" });
    assert.equal(areaOf(r), "overview");
    assert.equal(kindOf(`/stacks/new/${kind}`), kind);
  }
  assert.equal(kindOf("/stacks/new/preset/"), "preset");
});

test("ux-stacknew-2: the trail reads Stacks / New stack, and a stack named new keeps its hub", () => {
  const r = route("/stacks/new/bundle");
  assert.deepEqual(
    crumbs(r, "/stacks/new/bundle", "").map((c) => c.label),
    ["Stacks", "New stack"],
  );
  assert.deepEqual(route("/stacks/new"), {
    page: "stack",
    name: "new",
    tab: "overview",
  });
});
