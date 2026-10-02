// fix-216 (Kenny, 2026-10-02: "ik heb toch geen idee wat die snapshot is?"):
// the restore dialog's snapshot picker — every snapshot dated, aged, the
// newest preselected and labelled "latest" — as pure data, so the shape a
// browser draws is pinned without a DOM or a fetch.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  formatDateTimeBrussels,
  resolveSnapshotOwner,
  snapshotPickerRows,
} from "../js/snapshotpicker.js";

const DAY = 86400;

test("fix-216: the newest snapshot is first, preselected and labelled latest", () => {
  const now = 1_790_000_000;
  const snaps = [
    { id: "aaaa-long", short_id: "aaaa123", time: now - 3 * DAY },
    { id: "bbbb-long", short_id: "bbbb456", time: now - 1 * DAY },
    { id: "cccc-long", short_id: "cccc789", time: now - 7 * DAY },
  ];
  const rows = snapshotPickerRows(snaps, now);
  assert.equal(rows.length, 3);
  assert.equal(
    rows[0].shortId,
    "bbbb456",
    "newest first regardless of input order",
  );
  assert.equal(rows[0].latest, true);
  assert.equal(rows[0].selected, true, "the newest is preselected by default");
  assert.equal(
    rows[0].value,
    "",
    "latest's own value is the field's empty sentinel",
  );
  assert.equal(rows[1].latest, false);
  assert.equal(rows[1].selected, false);
  assert.equal(rows[2].ago, "7 days ago");
});

test("fix-216: an explicit preset snapshot id is the one preselected, not the newest", () => {
  const now = 1_790_000_000;
  const snaps = [
    { id: "aaaa-long", short_id: "aaaa123", time: now - 3 * DAY },
    { id: "bbbb-long", short_id: "bbbb456", time: now - 1 * DAY },
  ];
  const rows = snapshotPickerRows(snaps, now, "aaaa123");
  assert.equal(rows.find((r) => r.selected)?.shortId, "aaaa123");
  assert.equal(rows.find((r) => r.latest)?.selected, false);
});

test("fix-216: recent snapshots read in minutes and hours, not a bare day count", () => {
  const now = 1_790_000_000;
  const rows = snapshotPickerRows(
    [
      { id: "a", short_id: "a", time: now - 90 },
      { id: "b", short_id: "b", time: now - 5400 },
    ],
    now,
  );
  assert.match(rows[0].ago, /minutes? ago/);
  assert.match(rows[1].ago, /hours? ago/);
});

test("fix-216: a size and file count the host did not send stay null, never a guess", () => {
  const now = 1_790_000_000;
  const rows = snapshotPickerRows(
    [{ id: "a", short_id: "a", time: now - DAY }],
    now,
  );
  assert.equal(rows[0].size, null);
  assert.equal(rows[0].files, null);
});

test("fix-216: a size and file count the host does send are shown, humanised", () => {
  const now = 1_790_000_000;
  const rows = snapshotPickerRows(
    [
      {
        id: "a",
        short_id: "a",
        time: now - DAY,
        size_bytes: 5 * 1024 * 1024,
        file_count: 412,
      },
    ],
    now,
  );
  assert.equal(rows[0].size, "5.0 MB");
  assert.equal(rows[0].files, "412 files");
});

test("fix-216: the date reads dd/mm/yyyy HH:MM, 24-hour, fixed to Europe/Brussels", () => {
  // 2026-09-21 14:13:20 UTC == 16:13 in Europe/Brussels (CEST, +2)
  assert.equal(formatDateTimeBrussels(1_790_000_000), "21/09/2026 16:13");
});

test("fix-216: the picker reads a row's own app, never a different one with the same stack", () => {
  const repos = [{ owner: "kp-soft" }, { owner: "jobtracker" }];
  assert.equal(resolveSnapshotOwner(repos, "jobtracker")?.owner, "jobtracker");
  assert.equal(
    resolveSnapshotOwner(repos, "")?.owner,
    undefined,
    "ambiguous with no app chosen and more than one repo",
  );
});

test("fix-216: a single-repository stack (most native services) resolves without an app field at all", () => {
  const repos = [{ owner: "kyu" }];
  assert.equal(resolveSnapshotOwner(repos, "")?.owner, "kyu");
  assert.equal(resolveSnapshotOwner(repos, "anything-unmatched")?.owner, "kyu");
});

test("fix-223: the kind of backup the host tagged is shown; an untagged one stays null", () => {
  const now = 1_790_000_000;
  const rows = snapshotPickerRows(
    [
      { id: "a", short_id: "a", time: now - DAY, trigger: "nightly" },
      { id: "b", short_id: "b", time: now - 2 * DAY },
    ],
    now,
  );
  assert.equal(rows[0].kind, "nightly");
  assert.equal(rows[1].kind, null);
});
