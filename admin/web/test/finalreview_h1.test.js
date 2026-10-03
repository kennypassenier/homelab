// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04), H1:
// "a newer version" meant three things — Stacks and the hub flagged
// kp-soft and admin (demo-agent, a pin in the homelab binary), the Inbox
// and Update all listed only beta-demo's two apps, and Update kp-soft
// offered only its moving-tag apps. One source now says which apps have a
// newer version; every place counts the same apps, and the Update flow
// lists each one, an app that comes with a homelab release as a row that
// says so.
import { test } from "node:test";
import assert from "node:assert/strict";
import { newerApps, updateRows } from "../js/inboxrows.js";
import { newerByStack } from "../js/stacksview.js";
import { firstChosen, itemsFor } from "../js/updateflow.js";

/** The demo host's `/data/stale-images` shape. */
const body = {
  measured_at: 1_790_000_000,
  images: [
    {
      where_: "kp-soft/demo-agent",
      key: null,
      pinned: "v0.1.0",
      latest: "v0.2.0",
      upstream: "ghcr.io/x/demo-agent",
    },
    {
      where_: "admin/demo-agent",
      key: null,
      pinned: "v0.1.0",
      latest: "v0.2.0",
      upstream: "ghcr.io/x/demo-agent",
    },
    {
      where_: "beta-demo/api",
      key: "api/api",
      pinned: "v2.3.0",
      latest: "v3.0.0",
      upstream: "example/demo-api",
    },
    {
      where_: "beta-demo/web",
      key: "web/web",
      pinned: "1.4.2",
      latest: "1.5.0",
      upstream: "example/demo-web",
    },
  ],
};

test("redesign-final-h1: Stacks, the Inbox and the Update flow count the same apps with a newer version", () => {
  const apps = newerApps(body);
  assert.equal(apps.length, 4);
  const byStack = newerByStack(body.images);
  for (const s of ["kp-soft", "admin", "beta-demo"]) {
    const flow = itemsFor(body, { all: false, stack: s }).filter(
      (i) => i.kind !== "pull",
    );
    assert.equal(
      flow.length,
      byStack.get(s),
      `${s}: the flow lists ${flow.length}, Stacks flags ${byStack.get(s)}`,
    );
  }
  const all = itemsFor(body, { all: true }).filter((i) => i.kind !== "pull");
  assert.equal(all.length, 4);
  const [row] = updateRows(body);
  assert.equal(row.title, "4 apps have a newer version");
  assert.match(
    row.why,
    /kp-soft\/demo-agent v0\.1\.0 → v0\.2\.0 \(with a homelab release\)/,
  );
});

test("redesign-final-h1: an app that comes with a homelab release is listed, says so, and is never ticked", () => {
  const items = itemsFor(body, { all: false, stack: "kp-soft" });
  const rel = items.find((i) => i.container === "demo-agent");
  assert.ok(rel, "Update kp-soft does not list demo-agent");
  assert.equal(rel.kind, "release");
  assert.ok(!firstChosen(items, { all: false, stack: "kp-soft" }).has(rel.id));
  const one = itemsFor(body, {
    all: false,
    stack: "kp-soft",
    app: "demo-agent",
  });
  assert.ok(
    one.some((i) => i.kind === "release" && i.container === "demo-agent"),
    "an opener that picked demo-agent does not find it",
  );
});
