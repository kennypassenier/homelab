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

// ── feat-kpadopt-2 (kp-themes 11.0.0, the approved app-homelab-backups
// demo): the totals, the files reading, the trace and the lines' counts.
{
  const fwv = await import("../js/fwview.js");
  /** @param {Partial<import("../js/fwview.js").StackFw> & {stack: string}} s */
  const stack = (s) => ({
    vmid: 100,
    ip: "10.0.0.1/24",
    declared: true,
    enabled: true,
    policy_in: "DROP",
    policy_out: "DROP",
    rules: 0,
    management_open: null,
    live_enforced: true,
    live_matches_repo: true,
    ...s,
  });
  /** @type {import("../js/fwview.js").RuleRow} */
  const rule = {
    stack: "a",
    vmid: 100,
    n: 1,
    dir: "in",
    action: "ACCEPT",
    peer: "10.0.0.3",
    peer_stacks: ["c"],
    proto: "tcp",
    ports: "8080, 8787",
    note: "web",
    enabled: true,
  };
  /** @type {import("../js/fwview.js").FirewallRead} */
  const d = {
    stacks: [
      stack({ stack: "a", vmid: 101, rules: 2 }),
      stack({
        stack: "b",
        vmid: 102,
        declared: false,
        enabled: false,
        policy_in: null,
        policy_out: null,
        live_enforced: false,
        live_matches_repo: false,
      }),
      stack({
        stack: "c",
        vmid: 103,
        ip: "10.0.0.3/24",
        policy_out: "ACCEPT",
        live_enforced: false,
        live_matches_repo: false,
      }),
    ],
    matrix: {
      stacks: ["a", "b", "c"],
      unguarded: ["b"],
      rules: [
        rule,
        {
          ...rule,
          n: 2,
          dir: "out",
          stack: "a",
          peer: "10.0.0.2",
          peer_stacks: ["b"],
          ports: "",
        },
      ],
      cells: [
        {
          from: "a",
          to: "b",
          state: "open",
          allowed: [],
          stopped_at_source: [],
        },
        {
          from: "c",
          to: "b",
          state: "open",
          allowed: [],
          stopped_at_source: [],
        },
        {
          from: "c",
          to: "a",
          state: "none",
          allowed: [],
          stopped_at_source: [],
        },
        {
          from: "a",
          to: "c",
          state: "none",
          allowed: [],
          stopped_at_source: [],
        },
        {
          from: "b",
          to: "a",
          state: "none",
          allowed: [],
          stopped_at_source: [],
        },
        {
          from: "b",
          to: "c",
          state: "none",
          allowed: [],
          stopped_at_source: [],
        },
      ],
    },
  };

  test("feat-kpadopt-2: the totals name the stacks they count and the targets that let everything in", () => {
    const sets = fwv.fwSets(d);
    assert.deepEqual(sets.none, ["b"]);
    assert.deepEqual(sets.differs, ["c"]);
    assert.deepEqual(sets.openTargets, ["b"]);
  });

  test("feat-kpadopt-2: the files reading counts every declared firewall as in force; a pair differs where live and files disagree", () => {
    const files = fwv.cellIn(d, "c", "a", "files");
    assert.equal(files?.state, "some");
    assert.deepEqual(files?.allowed, ["tcp 8080", "tcp 8787"]);
    assert.equal(fwv.cellIn(d, "c", "a", "live")?.state, "none");
    assert.equal(fwv.pairDiffers(d, "c", "a"), true);
    assert.equal(fwv.pairDiffers(d, "b", "a"), false);
    assert.match(
      fwv.diffWords(d, "c", "a"),
      /Live: blocked\. Files: 8080, 8787\./,
    );
  });

  test("feat-kpadopt-2: the trace goes out of the source and in at the target and stops where a rule is missing", () => {
    const open = fwv.traceHops(d, "a", "b", "any");
    assert.deepEqual(
      open.map((x) => x.verdict),
      ["allowed", "allowed", "allowed", "allowed"],
    );
    assert.match(open[2].note ?? "", /no firewall in force/);
    const blocked = fwv.traceHops(d, "b", "a", "any");
    assert.equal(blocked[2].verdict, "refused");
    assert.equal(blocked[2].rule, "a in policy DROP");
  });

  test("feat-kpadopt-2: how many stacks a stack reaches and is reached by, and the ports in number order", () => {
    assert.deepEqual(fwv.reachCounts(/** @type {any} */ (d.matrix), "b"), {
      reaches: 0,
      reachedBy: 2,
    });
    assert.deepEqual(
      fwv.allPorts({
        .../** @type {any} */ (d.matrix),
        cells: [
          {
            from: "c",
            to: "a",
            state: "some",
            allowed: ["tcp 8787", "tcp 80"],
            stopped_at_source: [],
          },
        ],
      }),
      ["tcp 80", "tcp 8787"],
    );
  });
}
