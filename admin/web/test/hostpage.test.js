// redesign-host (3.71.0, the approved Host demo): the view models the
// redesigned Host page draws from — the KPI strip, the Containers table and
// its filter, the actions grouped by intent, the Disk card and host.toml
// shown as only what it changes.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionBlurb,
  diskBreakdown,
  guestTable,
  guestVisible,
  hostActionGroups,
  hostKpis,
  hostSettingsView,
  latencyBars,
  meterTone,
  rootGrowth,
  settingVisible,
} from "../js/host.js";

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

test("redesign-host: the KPI strip reads CPU, memory with its promise, root, pool and exact container counts", () => {
  const k = hostKpis(fleet(), guests);
  assert.deepEqual(
    k.map((x) => x.label),
    ["CPU", "Memory", "Root disk", "Container pool", "Containers"],
  );
  assert.equal(k[0].ctx, "16 cores · load 0.80");
  assert.equal(k[1].value, "25.4");
  assert.equal(k[1].unit, "of 64 GiB");
  assert.equal(k[1].ctx, "29.3 GiB promised to stacks (46%)");
  assert.equal(k[1].meter.mark, 46);
  assert.equal(k[2].ctx, "66 GB free of 96 GB");
  // The host sends the pool's size, not its use: never a made-up percent.
  assert.equal(k[3].value, "—");
  assert.equal(k[3].meter.pct, null);
  assert.equal(k[4].value, "3");
  assert.equal(k[4].unit, "of 4 running");
  assert.equal(k[4].ctx, "2 of 2 stacks online");
  // Before pct list answered: a dash, not "0 of 0".
  assert.equal(hostKpis(fleet(), null)[4].value, "—");
  assert.equal(meterTone({ pct: 90 }), "bad");
  assert.equal(meterTone({ pct: 75 }), "warn");
  assert.equal(meterTone({ pct: 90, neutral: true }), "");
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
