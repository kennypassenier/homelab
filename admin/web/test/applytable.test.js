// feat-kp11demo-13 (the approved apply demo): the plan as table rows, the
// sentence under it and the rule that a destroy never rides along.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  applyRule,
  plusMinus,
  planRows,
  sentenceCounts,
} from "../js/applytable.js";

/** @type {any} */
const plan = {
  deploy: ["jellyfin", "wiki2"],
  new: ["wiki2"],
  destroy: ["homepage"],
  broken: [["kp", "bad yaml :: fix stacks/kp"]],
  unchanged: ["traefik"],
  ephemeral: [],
};

test("feat-kp11demo-13: every stack of the plan is one row with its action", () => {
  const rows = planRows(plan, (s) => (s === "homepage" ? 117 : null));
  assert.deepEqual(
    rows.map((r) => `${r.stack}:${r.action}`),
    [
      "jellyfin:deploy",
      "wiki2:new",
      "homepage:destroy",
      "kp:broken",
      "traefik:unchanged",
    ],
  );
  assert.match(rows[2].note, /CT 117/);
  assert.equal(rows[3].fix, "fix stacks/kp");
});

test("feat-kp11demo-13: the sentence counts deploys, new ones and destroys of the ticks", () => {
  assert.deepEqual(sentenceCounts(plan, ["jellyfin", "wiki2"]), {
    deploy: 2,
    fresh: 1,
    destroy: 0,
  });
  assert.deepEqual(sentenceCounts(plan, ["homepage"]), {
    deploy: 0,
    fresh: 0,
    destroy: 1,
  });
});

test("feat-kp11demo-13: Apply is off for a destroy ridden along with a deploy, and with nothing ticked", () => {
  assert.match(applyRule(plan, ["jellyfin", "homepage"]), /never rides along/);
  assert.match(applyRule(plan, []), /Tick a stack/);
  assert.equal(applyRule(plan, ["jellyfin"]), "");
  assert.equal(applyRule(plan, ["homepage"]), "");
});

test("feat-kp11demo-13: lines added and removed add up over a stack's files", () => {
  assert.deepEqual(
    plusMinus(
      /** @type {any} */ ([
        { added: 2, removed: 1 },
        { added: 3, removed: 0 },
      ]),
    ),
    { add: 5, del: 1 },
  );
});
