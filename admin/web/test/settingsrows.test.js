// feat-kp11demo-5: which editor fits a host.toml key, and what the Save
// bar's text mirror of a row holds (settingsrows.js).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  boundsOf,
  editorId,
  editorKind,
  mirrorOf,
  shownValue,
  unitOf,
  valueFromMirror,
} from "../js/settingsrows.js";

/** @param {Record<string, unknown>} o */
const f = (o) =>
  /** @type {any} */ ({
    key: "k",
    group: "g",
    label: "K",
    help: "",
    default: "x",
    kind: { type: "text" },
    access: "browser",
    apply: "live",
    set: false,
    value: null,
    toml: null,
    ...o,
  });

test("each kind of key gets the editor that cannot be filled in wrongly", () => {
  assert.equal(editorKind(f({ kind: { type: "bool" } })), "switch");
  assert.equal(
    editorKind(
      f({ key: "backup_hour", kind: { type: "int", min: 0, max: 23 } }),
    ),
    "hour",
  );
  const n = { type: "int", min: 1, max: 16 };
  assert.equal(editorKind(f({ kind: n, default: "3" })), "quantity");
  // A default in words has no number to start the stepper from.
  assert.equal(editorKind(f({ kind: n, default: "not asked" })), "number");
  assert.equal(
    editorKind(f({ kind: n, default: "not asked", set: true, value: 4 })),
    "quantity",
  );
  assert.equal(editorKind(f({ kind: { type: "vmid_list" } })), "tags");
  assert.equal(editorKind(f({ kind: { type: "table" } })), "toml");
  assert.equal(editorKind(f({ access: "dashboard_secret" })), "secret");
  for (const a of ["ssh_only", "locked", "secret", "host_held"])
    assert.equal(editorKind(f({ access: a })), "read", a);
  assert.equal(editorKind(f({ access: "confirm" })), "text");
});

test("a seconds key reads in the biggest whole unit and back", () => {
  const k = f({
    key: "restic_snapshot_timeout_s",
    kind: { type: "int", min: 60, max: 4294967295 },
    default: "14400 (4 h)",
  });
  const u = unitOf(k);
  assert.equal(u.unit, "h");
  assert.equal(mirrorOf(k), "4");
  assert.deepEqual(boundsOf(k, u), { min: 1, max: 1193046 });
  assert.deepEqual(valueFromMirror(k, "5"), { ok: true, value: 18000 });
  const set = { ...k, set: true, value: 5400 };
  assert.equal(unitOf(set).unit, "min");
  assert.equal(mirrorOf(set), "90");
  assert.equal(shownValue(set, "90"), "90 min");
  assert.equal(shownValue(k, "1"), "1 h");
});

test("the nightly hour is a time of day; clearing it turns the round off", () => {
  const k = f({
    key: "backup_hour",
    kind: { type: "int", min: 0, max: 23 },
    default: "off",
    set: true,
    value: 3,
  });
  assert.equal(mirrorOf(k), "03:00");
  assert.deepEqual(valueFromMirror(k, "02:00"), { ok: true, value: 2 });
  assert.deepEqual(valueFromMirror(k, ""), { ok: true, value: null });
  assert.equal(valueFromMirror(k, "soon").ok, false);
});

test("lists, windows, switches and plain fields come back as the host takes them", () => {
  const ids = f({ kind: { type: "vmid_list" }, set: true, value: [100, 101] });
  assert.equal(mirrorOf(ids), "100,101");
  assert.deepEqual(valueFromMirror(ids, "100, 104"), {
    ok: true,
    value: [100, 104],
  });
  assert.equal(valueFromMirror(ids, "12").ok, false);
  assert.deepEqual(valueFromMirror(ids, ""), { ok: true, value: null });
  const w = f({ kind: { type: "window" }, default: "24h" });
  assert.equal(mirrorOf(w), "24");
  assert.deepEqual(valueFromMirror(w, "48"), { ok: true, value: "48h" });
  const sw = f({ kind: { type: "bool" }, set: true, value: true });
  assert.equal(mirrorOf(sw), "on");
  assert.deepEqual(valueFromMirror(sw, "off"), { ok: true, value: false });
  const url = f({ kind: { type: "url" } });
  assert.equal(valueFromMirror(url, "ftp://x").ok, false);
  assert.deepEqual(valueFromMirror(url, " https://x.y "), {
    ok: true,
    value: "https://x.y",
  });
  assert.deepEqual(valueFromMirror(url, ""), { ok: true, value: null });
  const num = f({ kind: { type: "int", min: 10, max: 100 }, default: "off" });
  assert.equal(valueFromMirror(num, "5").ok, false);
  assert.deepEqual(valueFromMirror(num, "50"), { ok: true, value: 50 });
});

test("a key's editor has a Live view field id", () => {
  assert.equal(editorId("backup_concurrency"), "key-backup-concurrency");
});
