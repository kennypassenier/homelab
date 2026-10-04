// redesign-final-12 (coordinator, 2026-10-04): a merge runs the
// whole-screen cases of the pages it changes. The mapping is derived from
// the code (main.js, the import graph, the cases' addresses); these pin
// that it finds the pages and the cases a change reaches, and no others.
import { test } from "node:test";
import assert from "node:assert/strict";
import { casesOf, pageModules, plan } from "../scripts/merge-cases.mjs";

/** The one layout walk. @param {string} c */
const isWalk = (c) => c.includes("redesign-final-gen: one walk");

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
  // redesign-final-50: a merge runs the one layout walk beside them.
  assert.ok(fw.cases.some(isWalk), "a merge without the layout walk");
  const ui = await plan(["admin/web/js/ui.js"]);
  // A shared file reaches the one layout walk, not every page's cases.
  assert.deepEqual(ui.pages, []);
  assert.deepEqual(
    ui.cases.filter((c) => !isWalk(c)),
    [],
  );
  assert.ok(ui.cases.some(isWalk), "no layout walk for a shared file");
  assert.equal((await plan(["docs/USER_GUIDE.md"])).cases.length, 0);
});

// redesign-final-49 (Kenny, 2026-10-04): a plain commit runs the cases of
// the pages its page files build; a shared file runs only the one layout
// walk, never every page's cases, and no other walker.
test("redesign-final-49: a commit's page file runs its page's cases; a shared file only the one layout walk", async () => {
  const fw = await plan(["admin/web/js/pages/firewall.js"], "commit");
  assert.deepEqual(fw.pages, ["firewall"]);
  assert.ok(fw.cases.length > 0);
  assert.ok(!fw.cases.some(isWalk), "the walk for a page file at commit");
  for (const f of [
    "admin/web/js/ui.js",
    "admin/web/js/dom.js",
    "admin/web/js/chrome.js",
  ]) {
    const x = await plan([f], "commit");
    assert.deepEqual(x.pages, [], f);
    assert.deepEqual(
      x.cases.filter((c) => !isWalk(c)),
      [],
      f,
    );
    assert.equal(x.cases.filter(isWalk).length, 1, f);
  }
  const css = await plan(["admin/web/css/app.css"], "commit");
  assert.deepEqual(css.pages, []);
  assert.ok(css.cases.some(isWalk));
});

test("redesign-final-48: the contrast case runs on a stylesheet or kp-themes change, never because a shared script changed", async () => {
  const isContrast = (/** @type {string} */ c) =>
    c.includes("redesign-final-contrast");
  const ui = await plan(["admin/web/js/ui.js"]);
  assert.ok(!ui.cases.some(isContrast), "contrast chosen for ui.js");
  const page = await plan(["admin/web/js/pages/firewall.js"]);
  assert.ok(!page.cases.some(isContrast));
  const css = await plan(["admin/web/css/app.css"]);
  assert.ok(css.cases.some(isContrast), "contrast not chosen for app.css");
  const pageCss = await plan(["admin/web/css/pages/firewall.css"], "commit");
  assert.ok(pageCss.cases.some(isContrast), "a page stylesheet at commit");
  const kp = await plan([], "merge", { kp: true });
  assert.deepEqual(kp.cases.filter(isContrast).length, 1, "a kp-themes bump");
  const cs = casesOf(`
test("invariants: themes", async () => {
  // merge-cases: on style change
  await page.goto(\`\${BASE}\${path}\`);
});
`);
  assert.deepEqual(cs, [
    { name: "invariants: themes", paths: [], style: true },
  ]);
});
