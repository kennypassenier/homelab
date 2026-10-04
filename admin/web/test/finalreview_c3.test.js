// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04), C3:
// the Notification rules page was never redesigned (numeric dates, a
// select for the snooze, the notice list the Inbox now holds). The rules'
// words and the demo's snooze segment.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  dayTime,
  digestWords,
  pushChip,
  snoozeChoices,
} from "../js/notifyrules.js";

// 2026-10-04 12:00 UTC, a Sunday.
const NOW = Date.UTC(2026, 9, 4, 12, 0) / 1000;

test("redesign-final-c3: a moment reads as the demos write it, never dd/mm/yyyy", () => {
  assert.equal(dayTime(NOW, "UTC"), "Sun 4 Oct, 12:00");
  assert.doesNotMatch(dayTime(NOW, "Europe/Brussels"), /\d+\/\d+\/\d+/);
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
    text: "Snoozed until Sun 4 Oct, 14:00",
    snoozed: true,
  });
  assert.equal(
    pushChip({ push: false, digest_at: null }, NOW).text,
    "Push off",
  );
});

test("redesign-final-c3: the snooze segment is the demo's 1 h · 4 h · Until 07:00 · 1 day", () => {
  const c = snoozeChoices(NOW);
  assert.deepEqual(
    c.map((x) => x.label),
    ["1 h", "4 h", "Until 07:00", "1 day"],
  );
  const morning = /** @type {{minutes: number}} */ (
    c.find((x) => x.value === "morning")
  );
  assert.ok(morning.minutes > 0 && morning.minutes <= 24 * 60);
  for (const x of c) assert.ok(x.hint, `${x.label} has no description`);
});

test("redesign-final-c3: the digest line says when and what came last, in words", () => {
  assert.equal(
    digestWords({ push: true, digest_at: null }, null),
    "No digest: only urgent pushes.",
  );
  assert.equal(
    digestWords(
      { push: true, digest_at: "07:30" },
      { at: NOW, count: 2 },
      "UTC",
    ),
    "One push at 07:30 with what still waits, worst first. Last one Sun 4 Oct, 12:00: 2 things waited.",
  );
});
