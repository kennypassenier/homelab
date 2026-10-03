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
  noIncidentsText,
  restartsDay,
  shortWhen,
  sizeFacts,
  staleFor,
  WHO_CHIPS,
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
      ["Restarts", "0", "", "reading the last 24 h…"],
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
      ["health:traefik", "not declared"],
      ["health:crowdsec", "not declared"],
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
  // redesign-integrate-6: a workstation's token names its owner, as
  // Activity's By does.
  assert.deepEqual(whoOf({ ...op, by: "wsl", req: 1 }), {
    key: "you",
    label: "Kenny · CLI on wsl",
  });
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

// ── senior review of redesign-stackhub (2026-10-03) ─────────────────────

test("review 1: an Env sealed the host did not measure reads as unknown, never as ok", () => {
  /** @param {any} v */
  const env = (v) =>
    healthChecks({
      s: stack({ env_sealed: v }),
      night: "ok",
      diskPct: 10,
      drift: null,
    }).find((r) => r.key === "env");
  assert.equal(env(true)?.verdict, "ok");
  assert.equal(env(false)?.verdict, "needs you");
  assert.equal(env(null)?.tone, "unknown");
  assert.equal(env(undefined)?.tone, "unknown");
});

/** A `/data/charts?range=24h` answer: 241 points 6 min apart ending at `to`. */
const chartsDay = (/** @type {Record<number, number>} */ perHourAgo) => {
  const to = 1_000_000;
  const step = 360;
  /** @type {[number, number][]} */
  const a = [];
  /** @type {[number, number][]} */
  const b = [];
  for (let i = 240; i >= 0; i--) {
    const t = to - i * step;
    // changes(…[1h]) at t counts the restarts in (t-1h, t]: a point that is
    // not on the hour sees part of two hours; only every 10th one is used.
    const h = Math.floor((to - t) / 3600);
    const onHour = (to - t) % 3600 === 0;
    a.push([t, onHour ? (perHourAgo[h] ?? 0) : 99]);
    b.push([t, onHour && h === 0 ? 1 : 0]);
  }
  return {
    from: to - 86400,
    to,
    step,
    panels: [
      { panel: { title: "CPU used" }, series: [] },
      {
        panel: { title: "Restarts (last hour)" },
        series: [
          { name: "web", points: a },
          { name: "db", points: b },
        ],
      },
    ],
  };
};

test("review 2: restarts in 24 h come from the hourly points aligned to the window end, oldest first", () => {
  const r = restartsDay(chartsDay({ 0: 2, 5: 1, 23: 3 }));
  assert.ok(r);
  assert.equal(r.hourly.length, 24);
  assert.equal(r.total, 2 + 1 + 1 + 3, "web 2+1+3 and db 1");
  assert.equal(r.hourly[23], 3, "the newest hour last");
  assert.equal(r.hourly[0], 3, "the oldest hour first");
  assert.equal(restartsDay({ panels: [] }), null, "no restarts panel");
  assert.equal(
    restartsDay({
      to: 1,
      step: 360,
      panels: [
        { panel: { title: "Restarts (last hour)" }, series: [], error: "down" },
      ],
    }),
    null,
    "a panel Prometheus failed is no reading",
  );
});

test("review 2: the Restarts tile counts 24 h with a sparkline and links to what shows restarts", () => {
  const base = {
    s: stack({
      apps: [
        { name: "traefik", running: true, restarts: 6 },
        { name: "crowdsec", running: true, restarts: 0 },
      ],
    }),
    stack: "gateway",
    last: null,
    night: "ok",
    errors: 0,
    drift: null,
    now: 1000,
  };
  const day = { total: 3, hourly: [...Array(23).fill(0), 3] };
  const t = hubKpis({ ...base, restarts24: day }).find(
    (k) => k.key === "restarts",
  );
  assert.equal(t?.value, "3");
  assert.equal(t?.ctx, "in the last 24 h");
  assert.deepEqual(t?.spark, day.hourly);
  assert.equal(t?.href, "/charts?stack=gateway&range=24h");
  // No Prometheus (or a native stack): the counter since each container
  // started, linked to the Apps tab that lists each app's restarts.
  const f = hubKpis({ ...base, restarts24: null }).find(
    (k) => k.key === "restarts",
  );
  assert.equal(f?.value, "6");
  assert.equal(f?.ctx, "since each container started");
  assert.equal(f?.tab, "apps");
  assert.equal(f?.href, undefined);
});

test("review 7: No restart loop counts the last 24 h: 0 ok, 1 and 4 need you, 5 and more failed", () => {
  /** @param {number} n */
  const loop = (n) =>
    healthChecks({
      s: stack({
        apps: [{ name: "traefik", running: true, restarts: 6 }],
        apps_total: 1,
        apps_running: 1,
      }),
      night: "ok",
      diskPct: 10,
      drift: null,
      restarts24: { total: n, hourly: [] },
    }).find((r) => r.key === "restarts");
  assert.deepEqual(
    [0, 1, 4, 5, 12].map((n) => loop(n)?.verdict),
    ["ok", "needs you", "needs you", "failed", "failed"],
  );
  assert.equal(loop(0)?.text, "No restart loop (0 restarts in 24 h)");
  assert.equal(loop(1)?.text, "No restart loop (1 restart in 24 h)");
  // Six restarts back in August no longer fail the stack forever: with a
  // 24 h reading the counter since creation is not used.
  assert.equal(loop(0)?.tone, "ok");
});

test("review 2: each app's own health check and its web addresses are rows of Is it healthy?", () => {
  const rows = healthChecks({
    s: stack({
      apps: [
        { name: "traefik", running: true, restarts: 0, health: "healthy" },
        { name: "crowdsec", running: true, restarts: 0, health: "unhealthy" },
        { name: "sidecar", running: true, restarts: 0 },
      ],
      apps_total: 3,
      apps_running: 3,
    }),
    night: "ok",
    diskPct: 10,
    drift: null,
    watch: [
      {
        key: "a",
        name: "Dashboard",
        stack: "gateway",
        state: "up",
        checked_at: 5,
      },
      { key: "b", name: "Films", stack: "films", state: "down", checked_at: 5 },
    ],
  });
  /** @param {string} k */
  const row = (k) => rows.find((r) => r.key === k);
  assert.equal(row("health:traefik")?.verdict, "healthy");
  assert.equal(row("health:traefik")?.tone, "ok");
  assert.equal(row("health:crowdsec")?.verdict, "unhealthy");
  assert.equal(row("health:crowdsec")?.tone, "bad");
  assert.equal(row("health:sidecar")?.verdict, "not declared");
  assert.equal(row("health:sidecar")?.tone, "info");
  assert.equal(row("web")?.text, "Its web addresses answer (1 watched)");
  assert.equal(row("web")?.tone, "ok");
  const down = healthChecks({
    s: stack(),
    night: "ok",
    diskPct: 10,
    drift: null,
    watch: [
      {
        key: "a",
        name: "Dashboard",
        stack: "gateway",
        state: "up",
        checked_at: 5,
      },
      { key: "c", name: "API", stack: "gateway", state: "down", checked_at: 5 },
    ],
  }).find((r) => r.key === "web");
  assert.equal(down?.tone, "bad");
  assert.equal(down?.verdict, "API down");
  const none = healthChecks({
    s: stack(),
    night: "ok",
    diskPct: 10,
    drift: null,
    watch: [],
  }).find((r) => r.key === "web");
  assert.equal(none?.verdict, "none watched");
  // A native unit: what systemctl is-active says.
  const nat = healthChecks({
    s: stack({
      native: true,
      apps: [{ name: "kyu", running: false, restarts: 0, health: "failed" }],
      apps_total: 1,
      apps_running: 0,
    }),
    night: "ok",
    diskPct: 10,
    drift: null,
  }).find((r) => r.key === "health:kyu");
  assert.equal(nat?.text, "kyu: systemctl is-active");
  assert.equal(nat?.verdict, "failed");
  assert.equal(nat?.tone, "bad");
});

test("review 2: the no-env row offers Push the env as its own host action", () => {
  const i = attentionItems({
    stack: "gateway",
    s: stack({ env_sealed: false }),
    now: 1,
  }).find((x) => x.key === "no-env");
  assert.ok(i);
  assert.equal(i.act.kind, "action");
  assert.equal(i.act.kind === "action" && i.act.action, "seal-env");
  assert.equal(i.act.label, "Push the env…");
  assert.equal(
    attentionItems({ stack: "gateway", s: stack({ env_sealed: null }), now: 1 })
      .length,
    0,
    "unknown is not a problem to fix",
  );
});

test("review 8: History says Kenny in the chip and in the rows", () => {
  assert.deepEqual(
    WHO_CHIPS.map((c) => c.label),
    ["Kenny", "Claude", "Nightly round"],
  );
  /** @type {any} */
  const op = {
    kind: "op",
    start: 1,
    end: 2,
    label: "deploy",
    ok: true,
    steps: [],
  };
  assert.equal(whoOf({ ...op, req: 3 }).label, "Kenny");
  assert.equal(whoOf({ ...op, by: "Kenny", req: 3 }).label, "Kenny");
});

test("review 9: History dates read as the demo's 30 Sep 12:14", () => {
  const t = Date.UTC(2026, 8, 30, 12, 14) / 1000;
  const now = Date.UTC(2026, 9, 3, 9, 0) / 1000;
  assert.equal(shortWhen(t, now, "UTC"), "30 Sep 12:14");
  assert.equal(
    shortWhen(Date.UTC(2026, 9, 2, 14, 22) / 1000, now, "UTC"),
    "2 Oct 14:22",
    "a day is not zero-padded",
  );
  assert.equal(
    shortWhen(Date.UTC(2025, 11, 30, 8, 5) / 1000, now, "UTC"),
    "30 Dec 2025 08:05",
    "another year says which",
  );
});

test("review 10: an empty incident list says no bundle is kept, not that nothing failed", () => {
  assert.equal(
    noIncidentsText("kp-soft"),
    "No incident bundles kept for kp-soft.",
  );
  assert.doesNotMatch(noIncidentsText("kp-soft"), /failed/);
});
