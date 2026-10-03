// redesign-stacks (3.71.0, the approved Stacks demo): the view model the
// redesigned Stacks page draws from — each stack's row (state, last backup,
// flags, its one action, its sparkline), the toolbar's search tokens and
// "Only" chips, the cards' sort, the host strip and the verdict.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  backupOf,
  deployCount,
  hostStrip,
  newerByStack,
  nextSort,
  onlyCounts,
  parseQuery,
  rowAction,
  rowMatches,
  series,
  sortRows,
  stackRows,
  verdict,
} from "../js/stacksview.js";

const NOW = 2_000_000;
const H = 3600;

/** @returns {any} a trimmed copy of the demo host's fleet */
const fleet = () => ({
  measured_at: NOW,
  counts: { stacks: 3, online: 2, parked: 1 },
  host: {
    name: "pve",
    cpu_pct: 7,
    ram_used_mb: 26000,
    ram_total_mb: 65536,
    disk_pct: 31,
    cores_total: 16,
    load1_x100: 80,
    disk_detail: { root_lv_size_gb: 96, root_disk_device: "/dev/sda" },
  },
  stacks: [
    {
      name: "gateway",
      vmid: 104,
      online: true,
      enabled: true,
      apps_running: 4,
      apps_total: 4,
      restarts: 1,
      ram_used_mb: 512,
      ram_max_mb: 1024,
      cpu_permille: 30,
      env_sealed: false,
      apps: [{ name: "traefik", running: true, restarts: 1 }],
    },
    {
      name: "kp-soft",
      vmid: 116,
      online: false,
      enabled: true,
      apps_running: 0,
      apps_total: 2,
      ram_used_mb: null,
      ram_max_mb: null,
      cpu_permille: null,
      env_sealed: true,
      apps: [],
    },
    {
      name: "films",
      vmid: 903,
      online: true,
      enabled: false,
      apps_running: 1,
      apps_total: 1,
      cpu_permille: 110,
      env_sealed: true,
      apps: [{ name: "jellyfin", running: true, restarts: 0 }],
    },
  ],
});

const calendar = {
  stacks: { gateway: [NOW - 2 * H, NOW - 26 * H], "kp-soft": [NOW - 50 * H] },
  no_backup: ["films"],
};

const rows = () =>
  stackRows(fleet(), {
    newer: newerByStack([
      { where_: "gateway/traefik", pinned: "3.1", latest: "3.2" },
      { where_: "gateway/whoami", pinned: "1", latest: "2" },
    ]),
    drift: { films: { state: "changed" }, gateway: { state: "same" } },
    calendar,
    trend: {
      step_s: 300,
      from: NOW,
      points: 4,
      host: { cpu_pct: [5, null, 9, 7], load_x100: [80, 90, null, 100] },
      stacks: { gateway: [null, 20, null, 40] },
    },
    now: NOW,
  });

test("redesign-stacks: newer versions are counted per stack from where_", () => {
  const m = newerByStack([
    { where_: "a/x", pinned: "1", latest: "2" },
    { where_: "a/y", pinned: "1", latest: "2" },
    { where_: "b/z", pinned: "1", latest: "2" },
  ]);
  assert.deepEqual(
    [...m],
    [
      ["a", 2],
      ["b", 1],
    ],
  );
});

test("redesign-stacks: the last backup says missed only past a night and a half, and never for a stack that keeps no data", () => {
  assert.equal(backupOf(null, "x", NOW).text, "reading…");
  assert.equal(backupOf(calendar, "films", NOW).state, "nodata");
  assert.equal(backupOf(calendar, "films", NOW).text, "keeps no data");
  const ok = backupOf(calendar, "gateway", NOW);
  assert.equal(ok.state, "ok");
  assert.equal(ok.at, NOW - 2 * H);
  assert.match(ok.text, /ago$/);
  const missed = backupOf(calendar, "kp-soft", NOW);
  assert.equal(missed.state, "missed");
  assert.equal(missed.tone, "bad");
  assert.match(missed.text, /^missed · /);
  assert.equal(backupOf(calendar, "other", NOW).text, "not read yet");
  assert.equal(
    backupOf({ stacks: { e: [] } }, "e", NOW).state,
    "none",
    "a stack read with no snapshot has no backup yet",
  );
});

test("redesign-stacks: a sparkline closes the trend's gaps with the last value and never invents a leading zero", () => {
  assert.deepEqual(series([null, 20, null, 40], 10), [2, 2, 4]);
  assert.deepEqual(series(undefined), []);
});

test("redesign-stacks: each row carries its state, numbers, flags, last backup and sparkline", () => {
  const [gw, kp, films] = rows();
  assert.equal(gw.state.label, "running");
  assert.equal(gw.newer, 2);
  assert.equal(gw.ramPct, 50);
  assert.equal(gw.cpuPct, 3);
  assert.deepEqual(gw.spark, [2, 2, 4]);
  assert.deepEqual(
    gw.flags.map((f) => f.key),
    ["newer", "noenv"],
  );
  assert.equal(gw.flags[0].label, "2 newer versions");
  assert.equal(kp.state.label, "offline");
  assert.equal(kp.problem, true, "an offline stack is a problem");
  assert.equal(kp.ramPct, null, "no RAM reading is never a made-up 0 %");
  assert.equal(films.drift, true);
  assert.equal(films.state.label, "parked");
  assert.deepEqual(
    films.flags.map((f) => f.key),
    ["drift", "parked"],
  );
  assert.equal(films.backup.text, "keeps no data");
});

test("redesign-stacks: a row offers the one action that fits it best", () => {
  const [gw, kp, films] = rows();
  assert.equal(kp.action.kind, "logs", "a down stack: read why");
  assert.equal(films.action.kind, "deploy", "differs from its files: deploy");
  assert.equal(gw.action.kind, "update", "a newer version: update");
  assert.equal(
    rowAction({
      drift: false,
      newer: 0,
      backup: { state: "missed", at: 1, text: "", tone: "bad" },
      state: { tone: "ok" },
    }).kind,
    "backup",
  );
  assert.equal(
    rowAction({
      drift: false,
      newer: 0,
      backup: { state: "ok", at: 1, text: "", tone: "ok" },
      state: { tone: "ok" },
    }).label,
    "Logs",
  );
});

test("redesign-stacks: the search takes words and state:/flag: tokens; Only chips add up", () => {
  const all = rows();
  const names = (/** @type {string} */ q, only = new Set()) =>
    all.filter((r) => rowMatches(r, parseQuery(q), only)).map((r) => r.name);
  assert.deepEqual(parseQuery(" Gate  state:run flag:noenv "), {
    words: ["gate"],
    state: ["run"],
    flag: ["noenv"],
  });
  assert.deepEqual(names("gate"), ["gateway"]);
  assert.deepEqual(names("jelly"), ["films"], "an app's name finds its stack");
  assert.deepEqual(names("903"), ["films"], "a vmid finds its stack");
  assert.deepEqual(names("state:offline"), ["kp-soft"]);
  assert.deepEqual(names("flag:noenv"), ["gateway"]);
  assert.deepEqual(names("", new Set(["newer"])), ["gateway"]);
  assert.deepEqual(names("", new Set(["newer", "problems"])), [
    "gateway",
    "kp-soft",
  ]);
  assert.deepEqual(names("", new Set(["drift"])), ["films"]);
  assert.deepEqual(onlyCounts(all), { newer: 1, problems: 1, drift: 1 });
});

test("redesign-stacks: the cards sort by vmid, name or busiest first, and the button cycles", () => {
  const all = rows();
  assert.deepEqual(
    sortRows(all, "name").map((r) => r.name),
    ["films", "gateway", "kp-soft"],
  );
  assert.deepEqual(
    sortRows(all, "cpu").map((r) => r.name),
    ["films", "gateway", "kp-soft"],
  );
  assert.deepEqual(
    sortRows(all, "vmid").map((r) => r.vmid),
    [104, 116, 903],
  );
  assert.equal(nextSort("vmid"), "name");
  assert.equal(nextSort("cpu"), "vmid");
});

test("redesign-stacks: the host strip has the demo's six tiles, each a link", () => {
  const t = hostStrip(
    fleet(),
    {
      step_s: 300,
      from: 0,
      points: 2,
      host: { cpu_pct: [5, 9], load_x100: [80, 120] },
      stacks: {},
    },
    { count: 3, urgent: 1 },
  );
  assert.deepEqual(
    t.map((x) => x.label),
    [
      "Stacks online",
      "CPU · 16 cores",
      "RAM",
      "Root disk",
      "Load (1 min)",
      "Needs you",
    ],
  );
  assert.ok(t.every((x) => x.href.startsWith("/")));
  const [online, cpu, ram, disk, load, inbox] = t;
  assert.equal(online.value, "2");
  assert.equal(online.unit, "of 3");
  assert.equal(online.ctx, "1 offline · 1 parked");
  assert.equal(online.tone, "bad");
  assert.deepEqual(cpu.spark, [5, 9]);
  assert.equal(ram.value, "25.4");
  assert.equal(ram.unit, "of 64 GB");
  assert.equal(ram.meter, 40);
  assert.equal(disk.ctx, "96 GB · /dev/sda");
  assert.deepEqual(load.spark, [0.8, 1.2]);
  assert.equal(load.value, "0.80");
  assert.equal(inbox.value, "3", "the exact count, never 9+");
  assert.equal(inbox.tone, "bad");
});

test("redesign-stacks: the verdict is absent when nothing waits, one sentence otherwise", () => {
  assert.equal(verdict([]), null);
  const one = verdict([{ title: "Backup missed", severity: "warn" }]);
  assert.deepEqual(one, {
    tone: "warn",
    title: "1 thing needs you",
    text: "Backup missed",
  });
  const many = verdict([
    { title: "gateway is down", severity: "bad" },
    { title: "x", severity: "warn" },
  ]);
  assert.equal(many?.tone, "bad");
  assert.equal(many?.title, "2 things need you");
  assert.equal(many?.text, "gateway is down · and 1 more");
});

test("redesign-stacks: Deploy all changes counts only what a comparison found", () => {
  assert.equal(deployCount(null), null);
  assert.equal(deployCount({ measured_at: null, stacks: {} }), null);
  assert.equal(
    deployCount({
      measured_at: 1,
      stacks: {
        a: { state: "changed" },
        b: { state: "same" },
        c: { state: "changed" },
      },
    }),
    2,
  );
});
