// The senior review of the 3.71.0 flows (2026-10-03) and the coordinator's
// destroy rule: Deploy all changes' subset and its separate destroy step,
// the one stylesheet loader, and the Update flow's own address, the moves
// it sends its one server job and the rows it shows from it.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import { destroyStep, goPlan } from "../js/applyplan.js";
import {
  flowArea,
  flowRows,
  legacyUpdateHref,
  undoWords,
  updatesBody,
} from "../js/updateflow.js";

const PLAN = {
  deploy: ["alpha", "beta"],
  new: [],
  destroy: ["oldstack", "other"],
  broken: /** @type {[string, string][]} */ ([["gamma", "did not parse"]]),
  unchanged: [],
  ephemeral: [],
};
const vmid = (/** @type {string} */ s) =>
  ({ oldstack: 905, other: 906 })[s] ?? null;

test("redesign-flows-5: Apply deploys the ticked subset and names what it leaves alone", () => {
  const g = goPlan(PLAN, new Set(["beta"]));
  assert.equal(g.label, "Apply 1 deploy…");
  assert.deepEqual(g.preset, { leave_out: "alpha, gamma" });
  assert.match(g.note, /alpha left out/);
  assert.match(g.note, /gamma cannot be planned/);
  assert.match(g.note, /Nothing is destroyed here/);
  const whole = goPlan({ ...PLAN, broken: [] }, new Set(["alpha", "beta"]));
  assert.deepEqual(whole.preset, {}, "the whole plan sends nothing extra");
  assert.equal(goPlan(PLAN, new Set()).n, 0);
});

test("redesign-flows-5: a destroy is its own step: armed, confirmed, after the deploys, with its CT number", () => {
  const off = destroyStep(PLAN, new Set(), vmid, {
    ack: true,
    deploysPending: false,
  });
  assert.equal(off.ready, false, "an unarmed destroy never runs");
  const armed = new Set(["oldstack"]);
  assert.equal(
    destroyStep(PLAN, armed, vmid, { ack: false, deploysPending: false }).ready,
    false,
    "not without its confirmation",
  );
  const waiting = destroyStep(PLAN, armed, vmid, {
    ack: true,
    deploysPending: true,
  });
  assert.equal(waiting.ready, false, "not while a ticked deploy waits");
  assert.match(/** @type {string} */ (waiting.why), /after the deploys/);
  const go = destroyStep(PLAN, armed, vmid, {
    ack: true,
    deploysPending: false,
  });
  assert.equal(go.ready, true);
  assert.deepEqual(go.preset, {
    leave_out: "alpha, beta, gamma",
    destroy: "oldstack",
    destroy_ids: "905",
    destroy_ack: true,
  });
  assert.equal(
    destroyStep(PLAN, armed, () => null, { ack: true, deploysPending: false })
      .ready,
    false,
    "not without the CT number the host records",
  );
});

test("review item 10: one stylesheet loader, in dom.js, and no copies", () => {
  /** @param {string} dir @returns {string[]} */
  const files = (dir) =>
    readdirSync(new URL(dir, import.meta.url), { withFileTypes: true }).flatMap(
      (e) =>
        e.isDirectory()
          ? files(`${dir}${e.name}/`)
          : e.name.endsWith(".js")
            ? [`${dir}${e.name}`]
            : [],
    );
  const copies = files("../js/").filter(
    (f) =>
      !f.endsWith("/dom.js") &&
      /createElement\("link"\)/.test(
        readFileSync(new URL(f, import.meta.url), "utf8"),
      ),
  );
  assert.deepEqual(copies, [], "a page or kit still loads a stylesheet itself");
  assert.match(
    readFileSync(new URL("../js/dom.js", import.meta.url), "utf8"),
    /export function ensureStyle\(href\)/,
  );
});

test("redesign-flows-11: the Update flow's own address, the old one sent on, and the area it belongs to", () => {
  assert.equal(legacyUpdateHref("?update=all"), "/update?all=1");
  assert.equal(
    legacyUpdateHref("?update=beta-demo&app=api%2Fapi"),
    "/update?stack=beta-demo&app=api%2Fapi",
  );
  assert.equal(legacyUpdateHref("?kind=update"), null);
  assert.equal(flowArea("?stack=kp-soft"), "overview");
  assert.equal(flowArea("?all=1"), "inbox");
});

test("redesign-flows-6: the moves the one job gets, and the rows it shows", () => {
  const pin = {
    id: "pin:beta-demo:api/api",
    kind: /** @type {const} */ ("pin"),
    stack: "beta-demo",
    container: "demo-api",
    key: "api/api",
    from: "v2.3.0",
    to: "v3.0.0",
    major: true,
    notes: null,
  };
  const pull = {
    ...pin,
    id: "pull:notes",
    kind: /** @type {const} */ ("pull"),
    stack: "notes",
    key: null,
  };
  const moves = new Map([
    [
      pin.id,
      {
        stack: "beta-demo",
        key: "api/api",
        file: "stacks/beta-demo/api/docker-compose.yml",
        from: "example/demo-api:v2.3.0@sha256:a",
        to: "example/demo-api:v3.0.0@sha256:b",
        from_version: "v2.3.0",
        to_version: "v3.0.0",
      },
    ],
  ]);
  assert.deepEqual(JSON.parse(updatesBody([pin, pull], moves).updates), [
    {
      stack: "beta-demo",
      kind: "pin",
      key: "api/api",
      app: "demo-api",
      from: "example/demo-api:v2.3.0@sha256:a",
      to: "example/demo-api:v3.0.0@sha256:b",
    },
    { stack: "notes", kind: "pull" },
  ]);
  const rows = flowRows(
    {
      rows: [
        {
          id: "backup",
          step: 3,
          title: "Back up",
          desc: "d",
          state: "ok",
          took_s: 9,
        },
        {
          id: "health",
          step: 5,
          title: "Verify",
          desc: "d",
          state: "bad",
          note: "1 of 2 running",
        },
      ],
    },
    { state: "ok", note: "0 errors" },
  );
  assert.deepEqual(
    rows.map((r) => [r.id, r.state]),
    [
      ["backup", "ok"],
      ["health", "bad"],
      ["logs", "ok"],
    ],
  );
  assert.equal(rows[0].time, "9 s");
  assert.deepEqual(undoWords(), {
    value: "Roll back, 1 click",
    ctx: "for 7 days, from the stack's History",
  });
});
