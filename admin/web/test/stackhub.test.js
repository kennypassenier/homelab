// redesign-stackhub (3.71.0, the approved stack hub demo): the view models
// the hub draws from — the header's chips and More menu, the stack's slice
// of Needs you, the KPI tiles, "Is it healthy?", who started what, the log
// filters, and the Apps and Backups rows.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  agoParts,
  appRows,
  attentionItems,
  backupRows,
  backupStanding,
  filterFeed,
  headerActions,
  headerChips,
  healthChecks,
  historyFeed,
  hubKpis,
  imageVersion,
  levelGroup,
  logSources,
  logView,
  logWindow,
  moreGroups,
  sizeFacts,
  staleFor,
  whoOf,
} from "../js/stackhub.js";

/** @returns {any} */
const stack = (over = {}) => ({
  name: "gateway",
  vmid: 104,
  online: true,
  enabled: true,
  apps_running: 2,
  apps_total: 2,
  env_sealed: true,
  native: false,
  apps: [
    { name: "traefik", running: true, restarts: 0 },
    { name: "crowdsec", running: true, restarts: 0 },
  ],
  ...over,
});

test("the header's chips: number, address, apps, and the flags that matter", () => {
  const chips = headerChips({
    s: stack({ env_sealed: false, enabled: false }),
    manifest: { network: { ip: "192.0.2.4/24" } },
    drift: { state: "changed", label: "changed" },
  });
  assert.deepEqual(
    chips.map((c) => c.label),
    ["vmid 104", "192.0.2.4", "2 apps", "parked", "files changed", "no env"],
  );
  assert.deepEqual(
    headerChips({ s: null, manifest: { apps: ["a"] } }).map((c) => c.label),
    ["1 app"],
  );
});

test("the header's slots back up and update the native way on a native stack", () => {
  assert.deepEqual(headerActions(true), {
    backup: "backup-native",
    update: "update-native",
    deploy: "deploy",
  });
  assert.equal(headerActions(null).backup, "backup");
});

test("More is grouped Data · Change · Pause · Tools · Remove and keeps every action", () => {
  const g = moreGroups({
    stack: "gateway",
    native: false,
    enabled: true,
    vmid: 104,
  });
  assert.deepEqual(
    g.map((x) => x.group),
    ["Data", "Change", "Pause", "Tools", "Remove"],
  );
  const labels = g.flatMap((x) => x.items.map((i) => i.label));
  for (const l of [
    "Restore…",
    "Change a secret…",
    "Roll back…",
    "Resize",
    "Add log guards",
    "Park this stack",
    "Export bundle",
    "Open the console",
    "Destroy, forget, wipe…",
  ])
    assert.ok(labels.includes(l), l);
  // a parked stack offers Unpark; a native one its own group
  const parked = moreGroups({ stack: "s", native: true, enabled: false });
  const pause = parked.find((x) => x.group === "Pause")?.items[0];
  assert.equal(pause?.kind === "action" && pause.action, "enable");
  assert.ok(parked.some((x) => x.group === "Native service"));
  // an older host that does not say: the native variants too (fix-229)
  const unknown = moreGroups({ stack: "s", native: null, enabled: true });
  const nat = unknown.find((x) => x.group === "Native service");
  assert.ok(
    nat?.items.some((i) => i.kind === "action" && i.action === "backup-native"),
  );
});

test("an age is an exact value and its unit", () => {
  assert.deepEqual(agoParts(12), { value: "12", unit: "s ago" });
  assert.deepEqual(agoParts(3600 + 59), { value: "1", unit: "h ago" });
  assert.deepEqual(agoParts(2 * 86400 + 5), { value: "2", unit: "d ago" });
});

test("needs you: worst first, each with its fix, and nothing when all is well", () => {
  assert.deepEqual(
    attentionItems({ stack: "gateway", s: stack(), now: 1_000_000 }),
    [],
  );
  const items = attentionItems({
    stack: "gateway",
    s: stack({
      env_sealed: false,
      apps_running: 1,
      apps: [
        { name: "traefik", running: false, restarts: 3 },
        { name: "crowdsec", running: true, restarts: 0 },
      ],
    }),
    night: "miss",
    last: 1_000_000 - 30 * 3600,
    now: 1_000_000,
    stale: staleFor("gateway", [
      {
        where_: "gateway/traefik",
        key: "traefik/traefik",
        pinned: "v3.4",
        latest: "v3.5",
      },
      { where_: "films/web", pinned: "1", latest: "2" },
    ]),
  });
  assert.deepEqual(
    items.map((i) => i.tone),
    ["bad", "bad", "warn", "info"],
  );
  assert.equal(items[0].title, "1 of 2 apps are not running");
  assert.equal(items[1].act.kind === "action" && items[1].act.action, "backup");
  assert.match(items[1].text, /the newest is 1 d old/);
  assert.equal(items[3].act.kind, "pin");
  // a stack the host does not run yet: Deploy creates it
  const nd = attentionItems({ stack: "beta", s: null, now: 1 });
  assert.equal(nd[0].act.kind === "action" && nd[0].act.action, "deploy");
});

test("the five KPI tiles", () => {
  const k = hubKpis({
    s: stack({ apps_running: 1 }),
    last: 1000 - 7200,
    night: "ok",
    errors: 2,
    drift: { state: "same", label: "same", measured_at: 994 },
    now: 1000,
  });
  assert.deepEqual(
    k.map((t) => [t.label, t.value, t.unit ?? "", t.ctx]),
    [
      ["Apps up", "1", "of 2", "1 stopped"],
      ["Restarts", "0", "", "since each container started"],
      ["Last backup", "2", "h ago", "last night covered"],
      ["Errors in logs · 1 h", "2", "", "read them"],
      ["Matches its files", "yes", "", "compared 6 s ago"],
    ],
  );
  const empty = hubKpis({
    s: stack(),
    last: null,
    night: "unknown",
    errors: null,
    drift: null,
    now: 1,
  });
  assert.equal(empty[3].ctx, "Loki did not answer");
  assert.equal(empty[4].value, "?");
});

test("is it healthy: every check with its verdict, manual checks last", () => {
  const rows = healthChecks({
    s: stack(),
    night: "miss",
    diskPct: 83,
    drift: null,
    manual: [
      {
        id: "c1",
        app: "traefik",
        text: "does it route?",
        answer: { label: "open", tone: "warn" },
      },
    ],
  });
  assert.deepEqual(
    rows.map((r) => [r.key, r.verdict]),
    [
      ["online", "ok"],
      ["apps", "ok"],
      ["restarts", "ok"],
      ["disk", "needs you"],
      ["backup", "failed"],
      ["env", "ok"],
      ["drift", "not measured"],
      ["manual:c1", "needs you"],
    ],
  );
});

test("who started an operation: a person, Claude, the nightly round", () => {
  /** @type {any} */
  const op = {
    kind: "op",
    start: 1,
    end: 2,
    label: "deploy",
    ok: true,
    steps: [],
  };
  assert.equal(whoOf({ ...op, by: "Kenny", req: 1 }).key, "you");
  assert.deepEqual(whoOf({ ...op, by: "Claude (Live view)", req: 1 }), {
    key: "claude",
    label: "Claude · Live view",
  });
  assert.equal(whoOf(op).key, "night");
  assert.equal(
    whoOf({ kind: "phase", start: 1, end: 2, name: "backup", count: 3 }).key,
    "night",
  );
});

test("the history feed reads in the past tense, newest first, and filters by chip and words", () => {
  /** @type {any[]} */
  const entries = [
    {
      kind: "op",
      start: 10,
      end: 20,
      label: "guards",
      by: "Claude (Live view)",
      req: 1,
      ok: true,
      steps: [],
    },
    {
      kind: "op",
      start: 30,
      end: 40,
      label: "update",
      by: "Kenny",
      req: 2,
      ok: false,
      error: "not healthy",
      steps: [],
    },
    { kind: "op", start: 50, end: 60, label: "backup", ok: true, steps: [] },
  ];
  const rows = historyFeed(entries);
  assert.deepEqual(
    rows.map((r) => [r.what, r.who.key, r.tone]),
    [
      ["Backed up", "night", "ok"],
      ["Update failed", "you", "bad"],
      ["Added log guards", "claude", "ok"],
    ],
  );
  assert.equal(filterFeed(rows, new Set(["claude", "night"]), "").length, 2);
  assert.deepEqual(
    filterFeed(rows, new Set(), "healthy update").map((r) => r.what),
    ["Update failed"],
  );
});

test("the Logs tab: windows, level groups, sources and the view", () => {
  assert.equal(logWindow("86400"), "86400");
  assert.equal(logWindow("21600"), "900");
  assert.equal(levelGroup("ERR"), "e");
  assert.equal(levelGroup("warning"), "w");
  assert.equal(levelGroup(""), "i");
  const lines = [
    { ts_ms: 3, source: "traefik", level: "error", line: "c" },
    { ts_ms: 1, source: "crowdsec", level: "info", line: "a" },
    { ts_ms: 2, source: "docker.service", level: "warn", line: "b" },
  ];
  assert.deepEqual(logSources(["traefik", "crowdsec"], lines), [
    "traefik",
    "crowdsec",
    "docker.service",
  ]);
  const v = logView(lines, new Set(["crowdsec"]), new Set(["w"]));
  assert.deepEqual(
    v.shown.map((l) => l.line),
    ["c"],
  );
  assert.deepEqual(v.levels, { i: 1, w: 1, e: 1 });
  assert.equal(v.sources.crowdsec, 1);
  assert.equal(v.total, 3);
});

test("the Apps rows: the pinned version, a newer one, a sidecar image", () => {
  assert.equal(imageVersion("traefik:v3.5@sha256:abc"), "v3.5");
  assert.equal(imageVersion("registry:5000/x/y"), "latest");
  const rows = appRows({
    s: stack(),
    images: { "traefik/traefik": "traefik:v3.5" },
    stale: staleFor("gateway", [
      {
        where_: "gateway/crowdsec",
        key: "crowdsec/crowdsec",
        pinned: "v1.6",
        latest: "v2.0",
      },
      { where_: "gateway/agent", key: null, pinned: "v0.1", latest: "v0.2" },
    ]),
  });
  assert.deepEqual(
    rows.map((r) => [
      r.name,
      r.version,
      r.newer?.latest ?? null,
      r.newer?.major ?? null,
      r.extra,
    ]),
    [
      ["traefik", "v3.5", null, null, false],
      ["crowdsec", "v1.6", "v2.0", true, false],
      ["agent", "v0.1", "v0.2", false, true],
    ],
  );
});

test("backups: the standing of last night and a 14-night strip per app", () => {
  const now = Date.UTC(2026, 9, 3, 12) / 1000;
  const night = (/** @type {number} */ n) => now - n * 86400 - 9 * 3600; // 03:00 of a night
  assert.equal(backupStanding({ times: [night(0)], now }).night, "ok");
  assert.equal(backupStanding({ times: [night(3)], now }).night, "miss");
  assert.equal(backupStanding({ times: null, now }).night, "unknown");
  assert.equal(
    backupStanding({ times: [], noBackup: true, now }).night,
    "none",
  );
  const rows = backupRows({
    now,
    repos: [
      {
        owner: "traefik",
        snapshot_count: 3,
        newest_snapshot: { id: "x", short_id: "a1", time: night(0) },
        snapshots: [0, 1, 3].map((n) => ({
          id: `${n}`,
          short_id: `${n}`,
          time: night(n),
        })),
      },
    ],
  });
  assert.equal(rows[0].cells.length, 14);
  assert.deepEqual(
    rows[0].cells.slice(-4).map((c) => c.state),
    ["ok", "miss", "ok", "ok"],
  );
  assert.equal(rows[0].cells[0].state, "before");
  assert.equal(rows[0].missed, 1);
  assert.equal(rows[0].newest?.short_id, "a1");
});

test("size and network, from the stack's files", () => {
  assert.deepEqual(
    sizeFacts({
      manifest: {
        resources: { memory_mb: 2048, cores: 2, disk_gb: 16 },
        network: { ip: "192.0.2.16/24" },
        firewall: { enabled: true, rules: [{}, {}] },
      },
      diskPct: 41.2,
    }).map((f) => `${f.label}: ${f.value}`),
    [
      "Memory: 2.0 GB",
      "Cores: 2",
      "Disk: 16 GB · 41% used",
      "Address: 192.0.2.16",
      "Firewall: on · 2 rules",
    ],
  );
  assert.deepEqual(sizeFacts({ manifest: null }), []);
});
