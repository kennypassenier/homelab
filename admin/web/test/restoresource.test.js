// restore-two-sources-1 (the owner: "I want to choose myself which of the two
// is restored"): the Restore flow's address, its default source and the
// arguments it sends all carry the source.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  defaultSource,
  restoreHref,
  restoreParams,
} from "../js/restoreflow.js";

test("restoreHref carries the source, and drops one it does not know", () => {
  assert.equal(
    restoreHref("media", "jellyfin", "a1b2", "local"),
    "/backups/restore?stack=media&app=jellyfin&snapshot=a1b2&source=local",
  );
  assert.equal(
    restoreHref("media", null, null, "remote"),
    "/backups/restore?stack=media&source=remote",
  );
  assert.equal(
    restoreHref("media", "db", null, "tape"),
    "/backups/restore?stack=media&app=db",
  );
  assert.equal(restoreHref("media"), "/backups/restore?stack=media");
});

test("defaultSource: Local when the host has it, else Remote, the address wins when usable", () => {
  assert.equal(defaultSource(true), "local");
  assert.equal(defaultSource(false), "remote");
  assert.equal(defaultSource(true, "remote"), "remote");
  assert.equal(defaultSource(true, "local"), "local");
  assert.equal(defaultSource(false, "local"), "remote");
  assert.equal(defaultSource(true, "nonsense"), "local");
});

test("restoreParams: compose and native restores both carry the source", () => {
  assert.deepEqual(
    restoreParams({
      native: false,
      stack: "media",
      app: "jellyfin",
      snapshot: "a1",
      source: "local",
    }),
    { confirm: "media", app: "jellyfin", snapshot: "a1", source: "local" },
  );
  assert.deepEqual(
    restoreParams({
      native: true,
      stack: "inbox",
      app: null,
      snapshot: "b2",
      source: "remote",
    }),
    { confirm: "inbox", snapshot: "b2", source: "remote" },
  );
});
