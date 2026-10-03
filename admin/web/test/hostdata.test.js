// redesign-host-4 (3.71.0): the seven facts the approved Host demo shows
// that the host daemon did not send — how full the thin pool is, what the
// guests are promised on it, unmanaged containers' memory and CPU, the
// host's uptime, the root disk's kind, whether the daemon is a signed
// release, and the growth line. Each is read from the host when it sends
// it, and said to be missing (never invented) when an older host does not.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  daemonSignature,
  diskBreakdown,
  guestTable,
  hostKpis,
  hostUptime,
  rootVolumeLine,
} from "../js/host.js";

/** @returns {any} a fleet reading from a host that sends every new fact */
const fleet = () => ({
  measured_at: 1,
  counts: { stacks: 1, online: 1, parked: 0 },
  host: {
    name: "pve",
    cpu_pct: 7,
    ram_used_mb: 26000,
    ram_total_mb: 65536,
    ram_committed_mb: 30000,
    disk_pct: 31,
    cores_total: 16,
    load1_x100: 80,
    uptime_s: 12 * 86400 + 3 * 3600 + 120,
    release: {
      signed: true,
      detail: "matches the SHA256SUMS signed with key 1C88AB06D43C0B16",
    },
    guests_usage: [
      {
        vmid: 104,
        cpu_permille: 30,
        ram_used_mb: 512,
        ram_max_mb: 1024,
        uptime_s: 10,
      },
      {
        vmid: 113,
        cpu_permille: 125,
        ram_used_mb: 3072,
        ram_max_mb: 8192,
        uptime_s: 10,
      },
    ],
    disk_detail: {
      root_lv_size_gb: 96,
      root_disk_device: "/dev/sda",
      root_disk_total_gb: 931.5,
      root_disk_kind: "SSD",
      thin_pool_size_gb: 780,
      thin_pool: {
        data_pct: 41,
        metadata_pct: 2.5,
        promised_gb: 1012.4,
        volumes: 9,
      },
      top_dirs: [["/var", 42]],
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
  ],
});

/** @returns {any} the same reading from a host older than 3.71.0 */
const oldFleet = () => {
  const f = fleet();
  delete f.host.uptime_s;
  delete f.host.release;
  delete f.host.guests_usage;
  delete f.host.disk_detail.thin_pool;
  delete f.host.disk_detail.root_disk_kind;
  return f;
};

const guests = [
  { vmid: 104, status: "running", lock: "", name: "104-app-gateway" },
  { vmid: 113, status: "running", lock: "", name: "113-metrics" },
  { vmid: 996, status: "stopped", lock: "template", name: "debian-13" },
];

test("redesign-host-4: the container pool KPI reads how full the thin pool is", () => {
  const pool = hostKpis(fleet(), guests)[3];
  assert.equal(pool.value, "41");
  assert.equal(pool.unit, "%");
  assert.equal(pool.ctx, "460 GB free of 780 GB");
  assert.equal(pool.meter.pct, 41);
  assert.match(pool.title ?? "", /metadata 2\.5%/);
  const old = hostKpis(oldFleet(), guests)[3];
  assert.equal(old.value, "—");
  assert.equal(old.meter.pct, null);
  assert.equal(old.ctx, "780 GB pool · use not reported by this host version");
});

test("redesign-host-4: the Disk card says what the guests are promised and what is really written", () => {
  const d = diskBreakdown(fleet());
  assert.ok(d?.pool);
  assert.equal(d.pool.promised, "1012 GB");
  assert.equal(d.pool.promisedNote, "130% of the pool, thin over-provisioned");
  assert.equal(d.pool.written, "320 GB · 41%");
  const old = diskBreakdown(oldFleet());
  assert.ok(old);
  assert.equal(old.pool, null);
});

test("redesign-host-4: the root volume line names the disk's kind, never a bare 'disk' when known", () => {
  assert.equal(rootVolumeLine(fleet()), "96 GB on /dev/sda (932 GB SSD)");
  assert.equal(rootVolumeLine(oldFleet()), "96 GB on /dev/sda (932 GB disk)");
});

test("redesign-host-4: an unmanaged container shows the memory and CPU the host measured", () => {
  const rows = guestTable(guests, fleet());
  assert.equal(rows[1].kind, "unmanaged");
  assert.equal(rows[1].ramUsed, 3072);
  assert.equal(rows[1].ramMax, 8192);
  assert.equal(rows[1].cpuPct, 13);
  // A stopped one stays a dash.
  assert.equal(rows[2].ramUsed, null);
  // An older host sends no per-guest use: a dash, as before.
  const old = guestTable(guests, oldFleet());
  assert.equal(old[1].ramUsed, null);
  assert.equal(old[1].cpuPct, null);
});

test("redesign-host-4: the host's uptime and the daemon's signature, or that this host version does not say", () => {
  assert.equal(hostUptime(fleet().host), "12 days 3 h");
  assert.equal(hostUptime(oldFleet().host), null);
  assert.deepEqual(daemonSignature(fleet().host), {
    text: "signed release",
    tone: "ok",
    title: "matches the SHA256SUMS signed with key 1C88AB06D43C0B16",
  });
  const unsigned = fleet().host;
  unsigned.release = { signed: false, detail: "installed without a signature" };
  assert.equal(daemonSignature(unsigned).text, "not a signed release");
  assert.equal(daemonSignature(unsigned).tone, "warn");
  assert.equal(
    daemonSignature(oldFleet().host).text,
    "signature not reported by this host version",
  );
});
