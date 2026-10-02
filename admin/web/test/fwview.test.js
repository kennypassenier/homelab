// fix-207 (Kenny: "Firewalls: 5 van de 11 staan aan … waarom zie ik dat
// dan niet op de topology van fleet view?"): `stackState` must show what
// the host actually enforces over what the repository merely declares,
// whenever the dashboard has an answer from the host at all.
import { test } from "node:test";
import assert from "node:assert/strict";
import { stackState } from "../js/fwview.js";

const base = {
  stack: "films",
  vmid: 110,
  ip: "10.10.10.10",
  declared: false,
  enabled: false,
  policy_in: null,
  policy_out: null,
  rules: 0,
  management_open: null,
};

test("no live answer: the repository's own declaration decides, as before fix-207", () => {
  assert.deepEqual(stackState({ ...base }), {
    label: "none declared",
    tone: "bad",
  });
  assert.deepEqual(stackState({ ...base, declared: true, enabled: false }), {
    label: "declared but off",
    tone: "warn",
  });
  assert.deepEqual(stackState({ ...base, declared: true, enabled: true }), {
    label: "in force",
    tone: "ok",
  });
});

test("the host enforces it: shown as in force even when the repository here declares nothing", () => {
  // Kenny's exact case: pve has 5 of 11 on, the repository's working copy
  // lags behind.
  assert.deepEqual(
    stackState({
      ...base,
      declared: false,
      enabled: false,
      live_enforced: true,
      live_matches_repo: false,
    }),
    { label: "in force (repo differs)", tone: "warn" },
  );
});

test("the host enforces it and the repository agrees: a plain in-force, no warning", () => {
  assert.deepEqual(
    stackState({
      ...base,
      declared: true,
      enabled: true,
      live_enforced: true,
      live_matches_repo: true,
    }),
    { label: "in force", tone: "ok" },
  );
});

test("the repository declares it enabled, but the host says it is not enforced: flagged, not silently 'in force'", () => {
  assert.deepEqual(
    stackState({
      ...base,
      declared: true,
      enabled: true,
      live_enforced: false,
      live_matches_repo: false,
    }),
    { label: "declared, not enforced", tone: "bad" },
  );
});

test("both sides agree nothing is enforced: back to the repository's own off states", () => {
  assert.deepEqual(
    stackState({
      ...base,
      declared: false,
      enabled: false,
      live_enforced: false,
      live_matches_repo: true,
    }),
    { label: "none declared", tone: "bad" },
  );
});
