// redesign-final (3.71.0's final whole-dashboard review, 2026-10-04): the
// logic behind the review's critical and high findings, one block per
// finding (the layout half lives in the whole-screen cases).
import { test } from "node:test";
import assert from "node:assert/strict";
// H4: the System landing was one flat grid of nine cards; the approved
// flows/system.html clusters them in four headed groups, with the host's
// live line on the Host card and the disaster runbook among the tools.
test("redesign-final-h4: System's cards sit in the demo's four groups, every System page once, plus the runbook", async () => {
  const { SYSTEM_GROUPS } = await import("../js/systemview.js");
  const { SUB_PAGES } = await import("../js/areas.js");
  assert.deepEqual(
    SYSTEM_GROUPS.map((g) => g.title),
    ["The host", "The fleet as a whole", "Set up", "Tools"],
  );
  for (const g of SYSTEM_GROUPS) assert.ok(g.desc, `${g.title}: no sentence`);
  const ids = SYSTEM_GROUPS.flatMap((g) => g.cards.map((c) => c.id));
  assert.deepEqual(
    ids.filter((i) => i !== "runbook").sort(),
    SUB_PAGES.filter((s) => s.area === "system")
      .map((s) => s.id)
      .sort(),
  );
  assert.ok(ids.includes("runbook"));
  assert.equal(
    SYSTEM_GROUPS[2].cards.find((c) => c.id === "sign-in")?.label,
    "Sign-in",
  );
});

test("redesign-final-h4: the Host card's live line reads CPU, disk and the version, with a newer release named", async () => {
  const { hostLine } = await import("../js/systemview.js");
  assert.deepEqual(
    hostLine(
      { host: { cpu_pct: 7.2, disk_pct: 31 } },
      { host: "3.63.0", latest: "3.64.0", update_available: true },
      false,
    ),
    {
      tone: "ok",
      word: "host healthy",
      line: "CPU 7% · disk 31% · 3.63.0 (3.64.0 available)",
    },
  );
  assert.equal(hostLine(null, null, true).word, "host not answering");
  assert.equal(hostLine(null, null, false).tone, "");
});
