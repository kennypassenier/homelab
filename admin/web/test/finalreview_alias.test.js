// redesign-final (3.71.0's final review round; coordinator, 2026-10-04):
// a control or field the C3 and H3 pages removed keeps its old name as a
// `was` alias, so `homelab ui click <old>` still lands, and the old
// addresses still redirect.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { resolve } from "../js/drivable.js";
import { redirectFor, route } from "../js/router.js";
import "../js/pages/inbox.js";
import "../js/pages/notifications.js";
import "../js/pages/restore.js";

test("redesign-final-c3/h3: every control or field the rules page and the Restore flow removed still resolves by its old name", () => {
  for (const [old, now] of [
    ["mark-read", "inbox-mark-seen"],
    ["open-notice", "inbox-see-what-happened"],
    ["notice-fix", "inbox-fix"],
    ["notify-snooze-minutes", "snooze-for"],
    ["bk-restore-stack", "restore-stack"],
    ["bk-restore-app", "restore-app"],
    ["bk-restore-snapshot", "restore-night"],
  ]) {
    const r = resolve(old);
    assert.ok(r, `ui click ${old} resolves to nothing`);
    assert.equal(r.control.id, now, `${old} resolves to ${r.control.id}`);
    assert.equal(r.was, old);
  }
  const cat = JSON.parse(
    readFileSync(new URL("../js/drivecatalog.json", import.meta.url), "utf8"),
  );
  const was = new Set(
    cat.controls.flatMap((/** @type {any} */ c) =>
      (c.was ?? []).map((/** @type {any} */ w) => w.id ?? w),
    ),
  );
  for (const old of ["mark-read", "notify-snooze-minutes", "bk-restore-app"])
    assert.ok(was.has(old), `the served catalog does not know ${old}`);
});

test("redesign-final-c3/h3: the old notices address redirects, the rules and the Restore flow have their own", () => {
  assert.equal(redirectFor(route("/notifications"), ""), "/inbox");
  assert.deepEqual(route("/system/notifications"), { page: "notifications" });
  assert.deepEqual(route("/backups/restore"), { page: "restore" });
});
