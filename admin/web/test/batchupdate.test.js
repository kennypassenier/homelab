// redesign-integrate-5 (3.71.0 consolidation): the Stacks page's batch
// Update… and the one Update flow (invariant 149, Kenny's decision "merge
// the two update mechanisms") met on merge: the batch opened one stack's
// flow and dropped the rest. The flow takes the ticked stacks as its scope.
import { test } from "node:test";
import assert from "node:assert/strict";

test("redesign-integrate-5: the Update flow takes several ticked stacks as one scope, and only their apps", async () => {
  const uf = /** @type {any} */ (await import("../js/updateflow.js"));
  assert.equal(typeof uf.updateHrefFor, "function", "no batch address");
  assert.equal(uf.updateHrefFor(["kp-soft"]), "/update?stack=kp-soft");
  const href = uf.updateHrefFor(["kp-soft", "beta-demo"]);
  assert.equal(href, "/update?all=1&only=kp-soft%2Cbeta-demo");
  const scope = uf.scopeOf(href.slice(href.indexOf("?")));
  assert.deepEqual(scope, { all: true, only: ["kp-soft", "beta-demo"] });
  assert.equal(uf.flowArea(href.slice(href.indexOf("?"))), "overview");
  assert.ok(uf.scopeMatches([{ stack: "beta-demo" }], scope));
  assert.ok(!uf.scopeMatches([{ stack: "films" }], scope));
  const stale = {
    images: [
      {
        where_: "kp-soft/web",
        key: "web/web",
        pinned: "a:1.0",
        latest: "a:1.1",
      },
      { where_: "films/jf", key: "jf/jf", pinned: "b:1.0", latest: "b:1.1" },
    ],
  };
  const items = uf.itemsFor(stale, scope);
  assert.deepEqual(
    items.map((/** @type {any} */ i) => i.stack),
    ["kp-soft"],
  );
  assert.ok(
    items.every((/** @type {any} */ i) => scope.only.includes(i.stack)),
    JSON.stringify(items),
  );
});
