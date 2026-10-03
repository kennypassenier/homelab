// drive-reach (Kenny, 2026-10-03: "the names are known, Claude made every
// button, guessing must stop"): the Live view control catalog the dashboard
// embeds and `homelab ui` checks every click and goto against is built from
// the declarations themselves, and the commit gate holds it so.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync } from "node:fs";
import {
  CATALOG_FILE,
  buildCatalog,
  catalogText,
} from "../scripts/drivecatalog.mjs";
import { controls, resolve } from "../js/drivable.js";
import { PATH_TO_PAGE, redirectFor, route } from "../js/router.js";

const built = await buildCatalog();

test("drive-reach: drivecatalog.json is exactly what the declarations build (run scripts/drivecatalog.mjs)", () => {
  assert.equal(
    readFileSync(CATALOG_FILE, "utf8"),
    catalogText(built),
    "admin/web/js/drivecatalog.json is stale: run `cd admin/web && node --import ./test/support/kp-register.mjs scripts/drivecatalog.mjs` and commit it",
  );
});

test("drive-reach: every page's controls are in the catalog, each once, with where it lives", () => {
  const c = /** @type {any} */ (built);
  const ids = c.controls.map((/** @type {any} */ x) => x.id);
  assert.equal(new Set(ids).size, ids.length, "an id is in the catalog twice");
  assert.ok(ids.length > 40, `only ${ids.length} controls`);
  for (const x of c.controls) {
    assert.ok(x.href?.startsWith("/"), `${x.id} has no address`);
    assert.ok(x.what, `${x.id} does not say what it does`);
  }
  // Every old name is unique across ids and old names, and resolves.
  const old = c.controls.flatMap((/** @type {any} */ x) =>
    x.was.map((/** @type {any} */ w) => w.id),
  );
  for (const w of old) {
    assert.ok(!ids.includes(w), `${w} is both an id and an old name`);
    assert.ok(resolve(w)?.was === w, `${w} does not resolve`);
  }
  assert.ok(old.includes("edit-schedule") && old.includes("delete-schedule"));
});

test("drive-reach: the catalog's redirect table is the router's own redirect, for every address", () => {
  const c = /** @type {any} */ (built);
  for (const [k, page] of Object.entries(PATH_TO_PAGE)) {
    assert.ok(c.addresses.includes(k), `/${k} is not drivable`);
    if (page !== "retired") continue;
    const want = redirectFor(route(`/${k}`), "", { stacks: ["s1"] });
    assert.equal(
      String(c.redirects[k]).replace("{stack}", "s1"),
      want,
      `/${k}`,
    );
  }
  for (const tab of ["checks", "firewall"])
    assert.equal(
      String(c.redirects[`stacks/{stack}/${tab}`]).replace("{stack}", "s1"),
      redirectFor(route(`/stacks/s1/${tab}`), ""),
    );
});

/** Every dashboard module's source, by its path under js/. */
function sources(dir = new URL("../js/", import.meta.url), prefix = "") {
  /** @type {[string, string][]} */
  const out = [];
  for (const e of readdirSync(dir, { withFileTypes: true })) {
    if (e.isDirectory())
      out.push(...sources(new URL(`${e.name}/`, dir), `${prefix}${e.name}/`));
    else if (e.name.endsWith(".js"))
      out.push([
        `${prefix}${e.name}`,
        readFileSync(new URL(e.name, dir), "utf8"),
      ]);
  }
  return out;
}

test("drive-reach: no page marks an element for Live view except through declare/drivable", () => {
  const declared = new Set(controls().map((c) => c.id));
  /** @type {string[]} */
  const bad = [];
  for (const [file, src] of sources()) {
    if (file === "drivable.js") continue;
    // A hand-written mark skips the declaration the registry needs.
    for (const m of src.matchAll(
      /dataset\.drive(?:Row)?\s*=(?!=)|["']data-drive(?:-row)?["']\s*:/g,
    ))
      bad.push(`${file}: writes ${m[0]} by hand (use drivable())`);
    // drivable(el, "literal") must name a declared control.
    for (const m of src.matchAll(/\bdrivable\(\s*[^,]+,\s*["']([^"']+)["']/g))
      if (!declared.has(m[1]))
        bad.push(`${file}: drivable(…, "${m[1]}") was never declared`);
  }
  assert.deepEqual(bad, [], bad.join("\n"));
});
