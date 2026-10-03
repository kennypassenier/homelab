// fix-241: the Backups page's "Show a file…" dialog — one file of a
// snapshot, read-only, capped at 1 MiB — as pure data.
import { test } from "node:test";
import assert from "node:assert/strict";
import { snapshotFileUrl, snapshotFileView } from "../js/snapshotfile.js";

test("fix-241: the route names stack, app, snapshot and an encoded path", () => {
  assert.equal(
    snapshotFileUrl("media", "jellyfin", "", "config/encoding.xml"),
    "/data/backups/media/jellyfin/latest/file?path=config%2Fencoding.xml",
  );
  assert.equal(
    snapshotFileUrl("media", "jellyfin", "af364ed7", "a b&c"),
    "/data/backups/media/jellyfin/af364ed7/file?path=a%20b%26c",
  );
});

test("fix-241: a whole text file is shown as it is, read only", () => {
  const v = snapshotFileView({
    owner: "jellyfin",
    snapshot: "af364ed7",
    path: "/appdata/media/jellyfin-config/config/encoding.xml",
    shown_bytes: 12,
    truncated: false,
    cap_bytes: 1048576,
    binary: false,
    text: "<x>vaapi</x>",
  });
  assert.equal(v.text, "<x>vaapi</x>");
  assert.equal(v.tone, "neutral");
  assert.equal(v.notes.length, 1);
  assert.match(v.notes[0], /nothing was restored/);
});

test("fix-241: a cut file says it was cut, a binary one shows nothing", () => {
  const cut = snapshotFileView({
    owner: "o",
    snapshot: "latest",
    path: "/p",
    shown_bytes: 1048576,
    truncated: true,
    cap_bytes: 1048576,
    binary: false,
    text: "aaa",
  });
  assert.equal(cut.tone, "warn");
  assert.match(cut.notes.join(" "), /1024 KiB/);
  const bin = snapshotFileView({
    owner: "o",
    snapshot: "latest",
    path: "/p.db",
    shown_bytes: 4096,
    truncated: false,
    cap_bytes: 1048576,
    binary: true,
    text: null,
  });
  assert.equal(bin.text, "");
  assert.match(bin.notes.join(" "), /Not text/);
});
