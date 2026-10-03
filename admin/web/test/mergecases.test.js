// redesign-final-12 (coordinator, 2026-10-04): a merge runs the
// whole-screen cases of the pages it changes. The mapping is derived from
// the code (main.js, the import graph, the cases' addresses); these pin
// that it finds the pages and the cases a change reaches, and no others.
import { test } from "node:test";
import assert from "node:assert/strict";
import { casesOf, pageModules, plan } from "../scripts/merge-cases.mjs";

test("redesign-final-12: main.js names each route page's module, views included", () => {
  const m = pageModules(`
import { mount as firewall } from "./pages/firewall.js";
import { mount as host } from "./pages/host.js";
    case "firewall":
      cleanup = firewall(page);
  doctor: (root) => host(root, { navigate, focus: "checks" }),
`);
  assert.equal(m.get("firewall"), "js/pages/firewall.js");
  assert.equal(m.get("doctor"), "js/pages/host.js");
});

test("redesign-final-12: a case's addresses come from its own body", () => {
  const cs = casesOf(`
test("invariants: one", async () => {
  await page.goto(\`\${BASE}/firewall\`);
});
test("invariants: two", async () => {
  for (const p of ["/backups", "/host?section=doctor"]) {}
  await page.route("**/data/doctor", () => {});
});
test(\`invariants: gen-\${cls}\`, async () => {});
`);
  assert.deepEqual(cs, [
    { name: "invariants: one", paths: ["/firewall"] },
    { name: "invariants: two", paths: ["/backups", "/host?section=doctor"] },
  ]);
});

test("redesign-final-12: a page module's change reaches its own page's cases and not another page's; a shared file reaches the cases that walk every page", async () => {
  const fw = await plan(["admin/web/js/pages/firewall.js"]);
  assert.deepEqual(fw.pages, ["firewall"]);
  assert.ok(
    fw.cases.some((c) => c.includes("redesign-final-c4")),
    "the Firewall row case is not chosen",
  );
  assert.ok(
    !fw.cases.some((c) => c.includes("redesign-final-h1")),
    "an Update flow case is chosen for a Firewall change",
  );
  const css = await plan(["admin/web/css/pages/firewall.css"]);
  assert.deepEqual(css.pages, ["firewall"]);
  const sched = await plan(["admin/web/js/pages/schedules.js"]);
  assert.ok(sched.pages.includes("activity"), "Planned is part of Activity");
  const ui = await plan(["admin/web/js/ui.js"]);
  assert.ok(ui.cases.length > fw.cases.length);
  assert.ok(
    ui.cases.some((c) => c.includes("title row puts the title left")),
    "a case that walks every page is not chosen for a shared file",
  );
  assert.equal((await plan(["docs/USER_GUIDE.md"])).cases.length, 0);
});
