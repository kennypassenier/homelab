// redesign-host (3.71.0, the approved Host demo): the view models the
// redesigned Host page draws from — the KPI tiles, the Containers table and
// its filter, the actions grouped by intent, the Disk card and host.toml
// shown as only what it changes.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionBlurb,
  diskBreakdown,
  guestTable,
  guestVisible,
  HOST_FIGURES,
  hostActionGroups,
  hostSettingsView,
  hostTile,
  latencyBars,
  rootGrowth,
  settingVisible,
} from "../js/host.js";
import { HOST_SECTIONS } from "../js/metricsview.js";

/** @returns {any} the demo host's own fleet reading, trimmed */
const fleet = () => ({
  measured_at: 1,
  counts: { stacks: 2, online: 2, parked: 0 },
  host: {
    name: "pve",
    cpu_pct: 7,
    ram_used_mb: 26000,
    ram_total_mb: 65536,
    ram_committed_mb: 30000,
    disk_pct: 31,
    cores_total: 16,
    load1_x100: 80,
    disk_detail: {
      root_lv_size_gb: 96,
      root_disk_device: "/dev/sda",
      root_disk_total_gb: 931.5,
      thin_pool_size_gb: 780,
      top_dirs: [
        ["/var", 42],
        ["/var/lib", 40],
        ["/usr", 18],
      ],
      measured_at: 1,
    },
  },
  stacks: [
    {
      name: "gateway",
      vmid: 104,
      online: true,
      enabled: true,
      apps_running: 1,
      apps_total: 1,
      ram_used_mb: 512,
      ram_max_mb: 1024,
      cpu_permille: 30,
    },
    {
      name: "films",
      vmid: 106,
      online: true,
      enabled: true,
      apps_running: 1,
      apps_total: 1,
    },
  ],
});

const guests = [
  { vmid: 104, status: "running", lock: "", name: "104-app-gateway" },
  { vmid: 106, status: "running", lock: "", name: "106-films" },
  { vmid: 113, status: "running", lock: "", name: "113-metrics" },
  { vmid: 996, status: "stopped", lock: "template", name: "debian-13" },
];

test("fix-371-1: the Host tiles are the eight figures, each with the 15-minute average as the big number", () => {
  assert.deepEqual(
    HOST_FIGURES.map((f) => f.key),
    ["cpu", "memory", "load", "disk", "diskio", "network", "temp", "swap"],
  );
  /** @param {number[]} vs @returns {[number, number][]} */
  const pts = (vs) => vs.map((v, i) => [1_000_000 + i * 600, v]);
  const fig = (/** @type {string} */ k) =>
    /** @type {import("../js/host.js").HostFigure} */ (
      HOST_FIGURES.find((f) => f.key === k)
    );
  const mem = hostTile(
    fig("memory"),
    {
      key: "memory",
      series: pts([30, 62, 40]),
      recent: pts([38, 40, 42, 44]),
    },
    fleet(),
  );
  assert.equal(mem.label, "Memory · avg 15 min");
  assert.equal(mem.value, "41");
  assert.equal(mem.unit, "%");
  assert.equal(mem.now, "44 %");
  assert.equal(mem.rest, "peak 62 % · 28.2 of 64 GiB");
  assert.equal(mem.href, "/charts#chart-mem");
  assert.equal(mem.points.length, 3);
  assert.equal(mem.from, 1_000_000);
  // A rate picks its unit by its size; load keeps two decimals.
  const net = hostTile(
    fig("network"),
    {
      key: "network",
      series: pts([12_000, 822_903]),
      recent: pts([12_000, 14_000]),
    },
    fleet(),
  );
  assert.equal(`${net.value} ${net.unit}`, "13 kB/s");
  assert.equal(net.rest, "peak 823 kB/s · in + out, the bridges");
  const io = hostTile(
    fig("diskio"),
    {
      key: "diskio",
      series: pts([2e6, 577e6]),
      recent: pts([14.9e6, 14.9e6]),
    },
    fleet(),
  );
  assert.equal(`${io.value} ${io.unit}`, "14.9 MB/s");
  assert.equal(io.rest, "peak 577 MB/s · read + written");
  // fix-371-1 follow-up: every tile lands on its own chart; Network on the
  // card with both directions it adds up.
  const swap = hostTile(
    fig("swap"),
    { key: "swap", series: pts([5]), recent: pts([5]) },
    fleet(),
  );
  assert.equal(swap.href, "/charts#chart-swap");
  assert.equal(swap.title, "Open Swap on Charts");
  assert.equal(io.href, "/charts#chart-diskio");
  assert.equal(net.href, "/charts#chart-netio");
  const cards = new Set(
    HOST_SECTIONS.flatMap((s) => s.cards).map((c) => c.key),
  );
  for (const f of HOST_FIGURES)
    assert.ok(f.chart && cards.has(f.chart), `${f.key} has no chart`);
  const load = hostTile(
    fig("load"),
    { key: "load", series: pts([0.2, 3.19]), recent: pts([0.84]) },
    fleet(),
  );
  assert.equal(load.value, "0.84");
  assert.equal(load.rest, "peak 3.19 · 16 cores");
});

test("fix-371-1: without a trend a Host tile shows the fleet's own reading as now, never a made-up average", () => {
  const cpu = /** @type {import("../js/host.js").HostFigure} */ (
    HOST_FIGURES[0]
  );
  const t = hostTile(cpu, null, fleet(), "the host's trends: no Prometheus");
  assert.equal(t.label, "CPU");
  assert.equal(t.value, "7");
  assert.equal(t.avg, false);
  assert.equal(t.rest, "now · no trend: Prometheus did not answer · 16 cores");
  assert.equal(t.points.length, 0);
  const swap = /** @type {import("../js/host.js").HostFigure} */ (
    HOST_FIGURES[7]
  );
  const s = hostTile(
    swap,
    { key: "swap", series: [], recent: [], error: "down" },
    fleet(),
  );
  assert.equal(s.value, "—");
  assert.match(s.rest, /^not measured · no trend/);
});

test("redesign-host: a container row carries its stack's measured memory and CPU, and its kind", () => {
  const rows = guestTable(guests, fleet());
  assert.equal(rows[0].stack, "gateway");
  assert.equal(rows[0].ramUsed, 512);
  assert.equal(rows[0].cpuPct, 3);
  // A managed stack not measured yet: no number, not 0.
  assert.equal(rows[1].ramUsed, null);
  assert.equal(rows[1].cpuPct, null);
  assert.equal(rows[2].kind, "unmanaged");
  assert.equal(rows[3].kind, "template");
  assert.equal(rows[3].running, false);
});

test("redesign-host: the running/stopped toggles and the search filter the containers", () => {
  const rows = guestTable(guests, fleet());
  const all = guestVisible(rows, { show: new Set(), q: "" });
  assert.deepEqual(all, [true, true, true, true]);
  assert.deepEqual(guestVisible(rows, { show: new Set(["stopped"]), q: "" }), [
    false,
    false,
    false,
    true,
  ]);
  // Both on is the same as all: each click turns one on or off.
  assert.deepEqual(
    guestVisible(rows, { show: new Set(["running", "stopped"]), q: "" }),
    all,
  );
  assert.deepEqual(guestVisible(rows, { show: new Set(), q: "GATE" }), [
    true,
    false,
    false,
    false,
  ]);
});

test("redesign-host: host actions are grouped by intent, and a new catalog action is never dropped", () => {
  /** @type {any[]} */
  const entries = [
    "patch",
    "exec",
    "template-build",
    "restart-host",
    "brand-new",
  ].map((action) => ({
    action,
    scope: action === "exec" || action === "restart-host" ? "all" : "operate",
    what: "x",
  }));
  entries.push({ action: "new-full", scope: "all", what: "x" });
  const g = hostActionGroups(entries);
  assert.deepEqual(
    g.map((x) => [x.title, x.actions.map((a) => a.action)]),
    [
      ["Keep it healthy", ["patch", "brand-new"]],
      ["Build and guard", ["template-build"]],
      ["Change the host", ["exec", "restart-host", "new-full"]],
    ],
  );
  assert.equal(g[2].full, true);
  assert.equal(
    actionBlurb(
      "run one shell command in a container (pct exec), audit-logged on the host; the host refuses unless",
    ),
    "Run one shell command in a container (pct exec), audit-logged on the host.",
  );
  assert.equal(
    actionBlurb("restarts the host daemon."),
    "Restarts the host daemon.",
  );
});

test("redesign-host: the Disk card stacks top-level directories once, and the growth line reads per week", () => {
  const d = diskBreakdown(fleet());
  assert.ok(d);
  assert.deepEqual(
    d.dirs.map((x) => x.path),
    ["/var", "/usr"],
  );
  assert.equal(d.freeGb, 66);
  assert.ok(d.dirs.reduce((s, x) => s + x.pct, 0) <= 100);
  assert.equal(rootGrowth([], 96), null);
  assert.equal(
    rootGrowth(
      [
        {
          scope: "host",
          subject: "/",
          fit: { pct_per_day_robust: 0.03, days_to_full: 1800 },
        },
      ],
      96,
    ),
    "Root grows 0.2 GB a week — full in about 5 years at this rate",
  );
  assert.equal(
    rootGrowth(
      [
        {
          scope: "host",
          subject: "/",
          fit: { pct_per_day_robust: 0, days_to_full: null },
        },
      ],
      96,
    ),
    "Root is not growing over the last week",
  );
});

test("redesign-host: host settings show only what host.toml changes by default, secrets never in clear", () => {
  const page = {
    fields: [
      {
        key: "backup_hour",
        label: "Nightly hour",
        group: "Nightly",
        default: "off",
        set: true,
        value: 3,
        access: "browser",
      },
      {
        key: "token",
        label: "Legacy token",
        group: "Access",
        default: "none",
        set: true,
        value: null,
        access: "secret",
      },
      {
        key: "log_level",
        label: "Log level",
        group: "Logs",
        default: "default: info",
        set: false,
        value: null,
        access: "browser",
      },
    ],
  };
  const v = hostSettingsView(page);
  assert.equal(v.changed, 2);
  assert.equal(v.rows[0].value, "3");
  assert.equal(v.rows[1].value, "set (not shown)");
  assert.equal(v.rows[2].value, "default");
  assert.equal(v.rows[2].def, "info");
  assert.deepEqual(settingVisible(v.rows, { all: false, q: "" }), [
    true,
    true,
    false,
  ]);
  assert.deepEqual(settingVisible(v.rows, { all: true, q: "log" }), [
    false,
    false,
    true,
  ]);
});

test("redesign-host: the latency strip scales to the slowest ping and says so", () => {
  const l = latencyBars([1, 2, 6]);
  assert.deepEqual(l.heights, [17, 33, 100]);
  assert.equal(l.label, "Round trip of the last 3 pings, the slowest 6 ms");
  assert.equal(latencyBars([]).label, "No ping yet");
});
