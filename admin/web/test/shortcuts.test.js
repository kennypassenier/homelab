// feat-overview-8: the keys, from the same list the help sheet shows.
import { test } from "node:test";
import assert from "node:assert/strict";
import { CHORD_MS, SHORTCUTS, idle, keyAction } from "../js/shortcuts.js";
import { route } from "../js/router.js";

const home = route("/app/");

test("g then a letter goes to a page, within the chord's time", () => {
  let r = keyAction(idle(), "g", 1000, home);
  assert.equal(r.action, null);
  assert.deepEqual(keyAction(r.state, "h", 1500, home).action, {
    navigate: "/app/host",
  });
  assert.deepEqual(keyAction(r.state, "k", 1000, home).action, {
    navigate: "/app/health",
  });
  // Too late: the second key is a key on its own.
  assert.equal(keyAction(r.state, "h", 1000 + CHORD_MS + 1, home).action, null);
  // An unknown second key does nothing and ends the chord.
  r = keyAction(r.state, "z", 1100, home);
  assert.equal(r.action, null);
  assert.deepEqual(r.state, idle());
});

test("/ searches; digits and brackets move between a stack's tabs", () => {
  assert.deepEqual(keyAction(idle(), "/", 0, home).action, {
    focusSearch: true,
  });
  const logs = route("/app/stacks/media/logs");
  assert.deepEqual(keyAction(idle(), "1", 0, logs).action, {
    navigate: "/app/stacks/media",
  });
  assert.deepEqual(keyAction(idle(), "[", 0, logs).action, {
    navigate: "/app/stacks/media/history",
  });
  assert.deepEqual(keyAction(idle(), "]", 0, logs).action, {
    navigate: "/app/stacks/media/checks",
  });
  assert.equal(keyAction(idle(), "4", 0, logs).action, null); // already there
  assert.deepEqual(keyAction(idle(), "7", 0, logs).action, {
    navigate: "/app/stacks/media/firewall",
  });
  assert.equal(keyAction(idle(), "9", 0, logs).action, null);
  assert.equal(
    keyAction(idle(), "]", 0, route("/app/stacks/media/firewall")).action,
    null,
  );
  // Digits mean nothing off a stack's page.
  assert.equal(keyAction(idle(), "1", 0, home).action, null);
});

test("the sheet lists every go-to key the handler knows", () => {
  const listed = SHORTCUTS.flatMap((g) => g.shortcuts.map((s) => s.keys));
  for (const k of ["o", "u", "k", "m", "h", "a", "f", "e"]) {
    assert.ok(listed.includes(`g ${k}`), k);
    const r = keyAction(keyAction(idle(), "g", 0, home).state, k, 1, home);
    assert.ok(r.action && "navigate" in r.action, k);
  }
  assert.ok(listed.includes("?"));
  assert.ok(listed.includes("Ctrl K"));
});
