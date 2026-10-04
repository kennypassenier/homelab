// redesign-final (Kenny, 2026-10-04): the whole-screen findings parked for
// 3.71.1 (test-e2e/parked.json). Each entry names a case that exists or a
// class the one layout walk reports (redesign-final-45), says its finding,
// its date and who parked it, and stands in docs/INVARIANTS.md; a parked
// case that passes fails, one still red does not.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { PARKED, parkedClass, parkedVerdict } from "../test-e2e/parked.js";

const e2e = readFileSync(
  new URL("../test-e2e/invariants.e2e.js", import.meta.url),
  "utf8",
);
const doc = readFileSync(
  new URL("../../../docs/INVARIANTS.md", import.meta.url),
  "utf8",
);

test("redesign-final: every parked case or layout class exists, says its finding and date, was parked by Kenny for 3.71.1, and is listed in docs/INVARIANTS.md", () => {
  assert.ok(PARKED.length > 0);
  for (const p of PARKED) {
    assert.ok(
      !!p.name !== !!p.class,
      `${JSON.stringify(p)}: a name or a class`,
    );
    if (p.name) {
      assert.ok(
        e2e.includes(`test(${JSON.stringify(p.name)}`),
        `no case named ${p.name}`,
      );
      assert.ok(
        doc.includes(p.name.replace(/^invariants: /, "")),
        `docs/INVARIANTS.md does not list ${p.name}`,
      );
    } else {
      assert.ok(
        new RegExp(`\\[\\s*"${p.class}",`).test(e2e),
        `the layout walk has no class ${p.class} (AUDIT_CLASSES)`,
      );
      assert.ok(
        doc.includes(`layout class \`${p.class}\``),
        `docs/INVARIANTS.md does not list layout class ${p.class}`,
      );
    }
    assert.match(p.date, /^\d{4}-\d{2}-\d{2}$/);
    assert.ok(p.finding.length > 10, `${p.name ?? p.class}: no finding`);
    assert.match(p.note, /^parked by Kenny \d{4}-\d{2}-\d{2} for 3\.71\.1$/);
  }
});

test("redesign-final: a parked case that passes fails the run; one still red passes", () => {
  const p = PARKED[0];
  assert.match(String(parkedVerdict(p, undefined)?.message), /take it off/);
  assert.equal(parkedVerdict(p, new Error("still red")), null);
});

test("redesign-final-45: the layout walk asks the list which classes are parked", () => {
  assert.equal(parkedClass("mono")?.class, "mono");
  assert.equal(parkedClass("a"), undefined);
  assert.match(e2e, /const parked = parkedClass\(cls\);/);
});
