// feat-overview-4 and the state column, driven without a browser.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  gb,
  hostCard,
  humanMb,
  measuredAgo,
  stackDetail,
  stackState,
} from "../js/fleet.js";

const base = { name: "media", vmid: 106, apps_running: 2, apps_total: 2 };

test("a parked stack reads parked even when it is offline", () => {
  assert.deepEqual(stackState({ ...base, online: false, enabled: false }), {
    label: "parked",
    tone: "warn",
  });
});

test("an online stack with an app down reads degraded", () => {
  assert.equal(
    stackState({ ...base, online: true, enabled: true, apps_running: 1 }).label,
    "degraded",
  );
});

test("measured ago uses seconds, then minutes, then hours", () => {
  assert.equal(measuredAgo(100, 112), "measured 12 s ago");
  assert.equal(measuredAgo(100, 100 + 125), "measured 2 min 5 s ago");
  assert.equal(measuredAgo(0, 3 * 3600 + 120), "measured 3 h 2 min ago");
  assert.equal(measuredAgo(200, 100), "measured 0 s ago");
});

test("ram reads in GB as a bare number, and a dash before the first reading", () => {
  assert.equal(gb(1946), "1.9");
  assert.equal(gb(5120), "5.0");
  assert.equal(gb(null), "—");
});

test("amounts read in the unit a person reads best", () => {
  assert.equal(humanMb(512), "512 MB");
  assert.equal(humanMb(16346), "16.0 GB");
  assert.equal(humanMb(31811), "31.1 GB");
  assert.equal(humanMb(4 * 1024 * 1024), "4.0 TB");
  assert.equal(humanMb(null), "—");
});

const fleet = {
  measured_at: 100,
  host: {
    name: "pve",
    cpu_pct: 7,
    ram_used_mb: 16343,
    ram_total_mb: 31811,
    disk_pct: 47,
  },
  counts: { stacks: 2, online: 2, parked: 0 },
  stacks: [
    {
      ...base,
      online: true,
      enabled: true,
      hostname: "media",
      ram_used_mb: 1713,
      ram_max_mb: 8192,
      uptime_s: 90061,
      applied_source: "a1b2c3d4e5f6",
      apps: [
        { name: "jellyfin", running: true, restarts: 0 },
        { name: "sonarr", running: false, restarts: 3 },
      ],
    },
    { ...base, name: "kyu", vmid: 109, online: true, enabled: true },
  ],
};

test("the host card reads RAM in GB and stacks as online of all", () => {
  const card = Object.fromEntries(
    hostCard(fleet).map((x) => [x.label, x.value]),
  );
  assert.equal(card.CPU, "7%");
  assert.equal(card.RAM, "16.0 GB of 31.1 GB");
  assert.equal(card["Stacks online"], "2 of 2");
  assert.equal(card.Disk, "47% used");
});

// fix-175: no fabricated "0%" before the host has a second /proc/stat
// sample to diff against.
test("the host card says 'not measured yet' when the host has no CPU reading", () => {
  const f = { ...fleet, host: { ...fleet.host, cpu_pct: null } };
  const card = Object.fromEntries(hostCard(f).map((x) => [x.label, x.value]));
  assert.equal(card.CPU, "not measured yet");
});

test("a stack page reads its facts and apps from the snapshot", () => {
  const d = stackDetail(fleet, "media");
  assert.ok(d);
  const facts = Object.fromEntries(d.facts.map((x) => [x.label, x.value]));
  assert.equal(facts.RAM, "1.7 GB of 8.0 GB");
  assert.equal(facts["Up for"], "1 day 1 h");
  assert.equal(facts["Deployed from"], "a1b2c3d4e5f6");
  assert.deepEqual(d.apps[1], {
    name: "sonarr",
    running: "stopped",
    tone: "bad",
    restarts: "3",
  });
});

test("a stack page says what it does not know yet", () => {
  const d = stackDetail(fleet, "kyu");
  assert.ok(d);
  const facts = Object.fromEntries(d.facts.map((x) => [x.label, x.value]));
  assert.equal(facts.RAM, "not measured yet");
  assert.equal(facts["Up for"], "not measured yet");
  assert.equal(facts["Deployed from"], "not recorded");
  assert.deepEqual(d.apps, []);
  assert.equal(stackDetail(fleet, "gone"), null);
  assert.equal(stackDetail(null, "media"), null);
});
