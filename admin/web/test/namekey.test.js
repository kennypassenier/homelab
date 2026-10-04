// redesign-final (coordinator, 2026-10-04): names that rows are matched by
// go through one key (namekey.js, the same as homelab_core::names::name_key),
// and every matcher finds a row whatever the name's spaces, dots, dashes,
// underscores, unicode or capitals.
import { test } from "node:test";
import assert from "node:assert/strict";
import { nameKey } from "../js/namekey.js";
import { incidentFor } from "../js/activityview.js";
import { stackIncidents } from "../js/stacktabs.js";
import { resolveSnapshotOwner } from "../js/snapshotpicker.js";

// The same table as core/src/names.rs names_tests::CASES.
const CASES = [
  ["update kp-soft", "update-kp-soft"],
  ["update-kp-soft", "update-kp-soft"],
  ["Backup  Gateway", "backup-gateway"],
  ["device-backup-OPNsense Router", "device-backup-opnsense-router"],
  ["wipe-kp-soft/jobtracker", "wipe-kp-soft-jobtracker"],
  ["app_v2.1", "app_v2.1"],
  [" -lead and trail- ", "lead-and-trail"],
  ["café über", "caf-ber"],
  ["", ""],
];

test("redesign-final: one name key, the same as the Rust side's", () => {
  for (const [name, key] of CASES) {
    assert.equal(nameKey(name), key, JSON.stringify(name));
    assert.equal(nameKey(key), key);
  }
});

test("redesign-final: every name-keyed matcher finds its row whatever the name's spelling", () => {
  const stacks = ["kp-soft", "Media Server", "app_v2.1", "Café"];
  for (const stack of stacks)
    for (const spell of [
      stack,
      stack.toUpperCase(),
      stack.replace(/-/g, " "),
    ]) {
      // An incident bundle named by the host's key, the entry by its words.
      const bundle = `1064-${nameKey(`update ${stack}`)}`;
      const r = /** @type {any} */ ({
        stack: spell,
        entry: {
          kind: "op",
          subject: `update ${spell}`,
          label: "update",
          start: 1000,
          end: 1064,
        },
      });
      assert.equal(incidentFor(r, [bundle]), bundle, `${stack} as ${spell}`);
      assert.deepEqual(
        stackIncidents([bundle, "9-backup-other"], spell),
        [bundle],
        `the hub of ${spell}`,
      );
      assert.equal(
        resolveSnapshotOwner(
          [
            { owner: stack, snapshots: [] },
            { owner: "x", snapshots: [] },
          ],
          spell,
        )?.owner,
        stack,
        `the snapshot owner ${spell}`,
      );
    }
});
