// check-row-answer-1 and inbox-resolved-clears-1 (Kenny, 2026-10-23): every
// Inbox row leads to where its fix is given, and a notice whose cause is
// gone is no row. Pure logic only: the rows and the notices as data.
import { test } from "node:test";
import assert from "node:assert/strict";
import { inboxItems } from "../js/inboxlist.js";
import { settledNotices } from "../js/inboxresolve.js";
import {
  countSplit,
  remedyTarget,
  rowFixes,
  todayRows,
  updateRows,
} from "../js/inboxrows.js";

/** @param {Partial<import("../js/notices.js").Notice>} o */
const notice = (o) =>
  /** @type {import("../js/notices.js").Notice} */ ({
    id: 1,
    at: 100,
    kind: "action_failed",
    title: "Back up media: failed",
    body: "it broke",
    read: false,
    push: { state: "sent" },
    level: "warning",
    ...o,
  });
const known = (/** @type {string} */ a) =>
  ["deploy", "backup", "restart", "install-native"].includes(a);

test("check-row-answer-1: every row kind carries an action that leads to its fix, in order, or says it has none", () => {
  /** @type {[string, import("../js/inboxrows.js").Row, any][]} */
  const cases = [];
  cases.push([
    "a host question",
    {
      key: "ask:b:1",
      severity: "bad",
      title: "t",
      why: "w",
      href: "/inbox",
      stack: null,
      at: 1,
      source: "asks",
    },
    {},
  ]);
  cases.push([
    "an update",
    updateRows({
      images: [
        {
          where_: "media/jellyfin",
          pinned: "1",
          latest: "2",
          upstream: "u",
          key: "j/j",
        },
      ],
    })[0],
    {},
  ]);
  const today = todayRows({
    today: {
      items: [
        {
          level: "Broken",
          source: "doctor",
          what: "a thing is off",
          remedy: "Run `homelab deploy media`.",
        },
      ],
    },
  })[0];
  cases.push(["a finding with a command in its remedy", today, {}]);
  cases.push([
    "a failure of a job, with its job",
    {
      key: "notice:5",
      severity: "warn",
      title: "Back up media: failed",
      why: "",
      href: "/inbox#notice-5",
      stack: "media",
      at: 1,
      source: "notices",
      job: 77,
    },
    {
      job: {
        action: "backup",
        stack: "media",
        state: "failed",
        args: { force: true, note: "x" },
      },
    },
  ]);
  const seen = [];
  for (const [name, row, ctx] of cases) {
    const f = rowFixes(row, { known, ...ctx });
    assert.ok(f.length > 0, `${name} has no action`);
    assert.notEqual(f[0].type, "none", `${name} has no real fix`);
    seen.push(f.map((x) => x.type).join("+"));
  }
  assert.deepEqual(seen, [
    "answer-ask",
    "update-flow",
    "run-fix",
    "open-job+run-fix",
  ]);
});

test("check-row-answer-1: a failed job's row offers its job and the same action again with its own arguments; a row with no known fix says so", () => {
  const base = {
    key: "notice:5",
    severity: /** @type {const} */ ("warn"),
    title: "x",
    why: "",
    href: "",
    stack: "media",
    at: 1,
    source: "notices",
  };
  const f = rowFixes(
    { ...base, job: 9 },
    {
      known,
      job: {
        action: "backup",
        stack: "media",
        state: "failed",
        args: { force: true, empty: "", n: 3, off: false },
      },
    },
  );
  const again = f.find((x) => x.type === "run-fix");
  assert.deepEqual(
    again && again.type === "run-fix"
      ? [again.fix.action, again.fix.args]
      : null,
    ["backup", { force: "true", n: "3" }],
  );
  // The job's action unknown to the dashboard: its job is still offered.
  assert.deepEqual(
    rowFixes(
      { ...base, job: 9 },
      { known, job: { action: "mystery", stack: "m", state: "failed" } },
    ).map((x) => x.type),
    ["open-job"],
  );
  // Nothing to point at: it says so, it is not only Snooze.
  const none = rowFixes({ ...base, href: "", source: "today" });
  assert.deepEqual(
    none.map((x) => x.type),
    ["none"],
  );
  assert.match(/** @type {any} */ (none[0]).note, /No fix to point to yet/);
});

test("check-row-answer-1: a remedy that names a command or a page leads to it", () => {
  const r = remedyTarget("Run it again: `homelab backup media`.", known);
  assert.deepEqual(
    r && r.type === "run-fix" ? [r.fix.action, r.fix.stack] : null,
    ["backup", "media"],
  );
  assert.equal(remedyTarget("Run `homelab nonsense media`.", known), null);
  const p = remedyTarget("Open the Backups page and pick the night.", known);
  assert.deepEqual(p && p.type === "link" ? p.href : null, "/backups");
  assert.equal(
    remedyTarget("open its page, look at its logs and restart it", known),
    null,
  );
  // A command without a stack uses the row's own.
  const own = remedyTarget("Run `homelab restart`.", known, "kitchen");
  assert.equal(own && own.type === "run-fix" ? own.fix.stack : null, "kitchen");
});

test("inbox-resolved-clears-1: a notice a later event settled is no Inbox row", () => {
  const n = [
    // "does not answer" then "answers again", six times over (a key each).
    ...[1, 2, 3].flatMap((i) => [
      notice({
        id: i * 10,
        at: i * 100,
        kind: "down",
        level: "critical",
        title: `Sonarr${i} does not answer`,
        key: `k${i}`,
      }),
      notice({
        id: i * 10 + 1,
        at: i * 100 + 50,
        kind: "up",
        level: "info",
        title: `Sonarr${i} answers again`,
        key: `k${i}`,
      }),
    ]),
    // Still down: stays.
    notice({
      id: 40,
      at: 400,
      kind: "down",
      level: "critical",
      title: "Radarr does not answer",
      key: "radarr",
    }),
    // A failed backup, then a good one (host events, no key, same op).
    notice({
      id: 50,
      at: 500,
      kind: "host_event",
      stack: "paperwork",
      op: "backup-paperwork",
      title: "scheduled-backup paperwork failed",
    }),
    notice({
      id: 51,
      at: 600,
      kind: "host_event",
      stack: "paperwork",
      op: "backup-paperwork",
      level: "ok",
      title: "scheduled-backup paperwork: done",
    }),
    // The same, older data with no op: the title says what it is about.
    notice({
      id: 60,
      at: 500,
      kind: "action_failed",
      stack: "kitchen",
      title: "Install newest kitchen: failed",
    }),
    notice({
      id: 61,
      at: 700,
      kind: "action_done",
      stack: "kitchen",
      level: "ok",
      title: "Install newest kitchen: done",
    }),
    // Another stack's failure is not settled by kitchen's success.
    notice({
      id: 62,
      at: 500,
      kind: "action_failed",
      stack: "gateway",
      title: "Install newest gateway: failed",
    }),
    // A failure AFTER the success stays.
    notice({
      id: 63,
      at: 800,
      kind: "action_failed",
      stack: "kitchen",
      title: "Install newest kitchen: failed",
    }),
    // Nightly checks: the older one is superseded by the newer.
    notice({
      id: 70,
      at: 500,
      kind: "fleet_check",
      title: "Nightly check: 3 findings",
    }),
    notice({
      id: 71,
      at: 900,
      kind: "fleet_check",
      title: "Nightly check: 1 finding",
    }),
  ];
  const settled = settledNotices(n);
  assert.deepEqual(
    [...settled.keys()].sort((a, b) => a - b),
    [10, 20, 30, 50, 60, 70],
  );
  const rows = inboxItems({ now: 1000, notices: n });
  assert.deepEqual(
    rows.map((r) => r.key).sort(),
    ["notice:40", "notice:62", "notice:63", "notice:71"].sort(),
  );
  // The host's own settling (`resolved_by`) counts too.
  assert.equal(
    settledNotices([notice({ id: 1, resolved_by: 2, read: false })]).get(1),
    2,
  );
});

test("inbox-resolved-clears-1: the Inbox header and 'needs you' use one count and show how it splits", () => {
  const rows = /** @type {any[]} */ ([
    { key: "a", severity: "bad", source: "notices", kind: undefined },
    { key: "b", severity: "warn", source: "today", kind: "check" },
    { key: "c", severity: "warn", source: "updates", kind: "update" },
    { key: "d", severity: "warn", source: "updates", kind: "update" },
  ]);
  const s = countSplit(rows);
  assert.deepEqual([s.total, s.act, s.updates], [4, 2, 2]);
  assert.equal(s.text, "2 to deal with · 2 updates");
  assert.equal(countSplit([]).text, "");
});
