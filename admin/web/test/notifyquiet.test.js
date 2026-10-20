import assert from "node:assert/strict";
import { test } from "node:test";
import { pushVerdict, quietCovers, quietWords } from "../js/notifyrules.js";

// Tuesday 20 October 2026, Brussels summer time (UTC+2).
/** @param {number} day @param {number} hour @param {number} minute */
const at = (day, hour, minute) => Date.UTC(2026, 9, day, hour, minute) / 1000;
const quiet = { from: "22:00", to: "07:00", days: [2], critical_breaks: true };

test("a window across midnight belongs to the day it starts on", () => {
  assert.equal(quietCovers(quiet, at(20, 21, 30)), true);
  assert.equal(quietCovers(quiet, at(21, 1, 0)), true);
  assert.equal(quietCovers(quiet, at(21, 21, 30)), false);
  assert.equal(quietCovers(quiet, at(20, 8, 0)), false);
  assert.equal(quietWords(quiet), "22:00 to 07:00 on Tue");
});

test("quiet hours and digest-only severities hold a push; critical may break through", () => {
  const rules = {
    push: true,
    quiet_hours: quiet,
    routing: { critical: true, warning: true, info: false, ok: true },
  };
  const night = at(20, 21, 30);
  assert.equal(pushVerdict({ level: "warning" }, rules, night).pushed, false);
  assert.equal(pushVerdict({ level: "critical" }, rules, night).pushed, true);
  const strict = {
    ...rules,
    quiet_hours: { ...quiet, critical_breaks: false },
  };
  assert.match(
    pushVerdict({ level: "critical" }, strict, night).why,
    /quiet hours/,
  );
  const digestOnly = {
    ...rules,
    quiet_hours: null,
    routing: { ...rules.routing, critical: false },
  };
  assert.match(
    pushVerdict({ level: "critical" }, digestOnly, night).why,
    /digest only/,
  );
});
