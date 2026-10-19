// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04), C3:
// the Notification rules page was never redesigned (numeric dates, a
// select for the snooze, the notice list the Inbox now holds). The rules'
// words and the demo's snooze segment.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  SNOOZE_MAX_MINUTES,
  dayTime,
  digestLast,
  pushChip,
  pushVerdict,
  snoozeMinutes,
} from "../js/notifyrules.js";

// 2026-10-04 12:00 UTC, a Sunday.
const NOW = Date.UTC(2026, 9, 4, 12, 0) / 1000;

test("redesign-final-c3: a moment reads as the demos write it, never dd/mm/yyyy", () => {
  assert.equal(dayTime(NOW, "UTC"), "04/10/2026 12:00");
  assert.match(
    dayTime(NOW, "Europe/Brussels"),
    /^\d{2}\/\d{2}\/\d{4} \d{2}:\d{2}$/,
  );
});

test("redesign-final-c3: the push chip says on, snoozed until when, or off", () => {
  assert.equal(
    pushChip({ push: true, digest_at: "07:30" }, NOW, "UTC").text,
    "Push on · not snoozed",
  );
  const s = pushChip(
    { push: true, digest_at: null, snooze_until: NOW + 7200 },
    NOW,
    "UTC",
  );
  assert.deepEqual(s, {
    tone: "warn",
    text: "Snoozed until 04/10/2026 14:00",
    snoozed: true,
  });
  assert.equal(
    pushChip({ push: false, digest_at: null }, NOW).text,
    "Push off",
  );
});

test("kp11: a snooze is the minutes to its moment, at least one and at most the week the server takes", () => {
  const ms = NOW * 1000;
  assert.equal(snoozeMinutes(ms + 3_600_000, ms), 60);
  assert.equal(snoozeMinutes(ms + 3_600_001, ms), 61);
  assert.equal(snoozeMinutes(ms + 1, ms), 1);
  assert.equal(snoozeMinutes(ms - 5000, ms), 1);
  assert.equal(snoozeMinutes(ms + 9 * 86_400_000, ms), SNOOZE_MAX_MINUTES);
  assert.equal(SNOOZE_MAX_MINUTES, 7 * 24 * 60);
});

test("kp11: the last digest is said in words under the digest's time", () => {
  assert.equal(digestLast(null), "None sent yet.");
  assert.equal(
    digestLast({ at: NOW, count: 2 }, "UTC"),
    "Last one 04/10/2026 12:00: 2 things waited.",
  );
  assert.equal(
    digestLast({ at: NOW, count: 1 }, "UTC"),
    "Last one 04/10/2026 12:00: 1 thing waited.",
  );
});

test("kp11: the rules would push an urgent notice at once and hold the rest for the digest, in the server's order", () => {
  const rules = { push: true, digest_at: "09:00", muted_stacks: ["loki"] };
  const urgent = { level: "critical", stack: "vaultwarden" };
  assert.equal(pushVerdict(urgent, rules, NOW, "UTC").pushed, true);
  assert.equal(
    pushVerdict({ level: "warning", stack: "immich" }, rules, NOW, "UTC").why,
    "Held for the 09:00 digest: only urgent notices push at once.",
  );
  assert.match(
    pushVerdict({ level: "info" }, { push: true }, NOW).why,
    /no daily digest/,
  );
  assert.equal(
    pushVerdict({ level: "critical", stack: "loki" }, rules, NOW).why,
    "Not pushed: loki is muted.",
  );
  assert.equal(
    pushVerdict(urgent, { ...rules, push: false }, NOW).why,
    "Not pushed: push to the phone is off.",
  );
  // A snooze silences everything, even an urgent notice on a muted stack.
  assert.equal(
    pushVerdict(
      { level: "critical", stack: "loki" },
      { ...rules, snooze_until: NOW + 7200 },
      NOW,
      "UTC",
    ).why,
    "Not pushed: every push is snoozed until 04/10/2026 14:00.",
  );
});
