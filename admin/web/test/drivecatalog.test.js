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
import {
  closest,
  controls,
  declare,
  fields,
  pick,
  resolve,
} from "../js/drivable.js";
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

/**
 * The top-level arguments of every call of `name(` in `src`, as source
 * text: nested calls, objects, arrays, strings and template literals keep
 * their own commas (review M2: a split on the first comma broke on them).
 * @param {string} src
 * @param {string} name
 * @returns {{at: number, def: boolean, args: string[]}[]}
 */
export function calls(src, name) {
  /** @type {{at: number, def: boolean, args: string[]}[]} */
  const out = [];
  const re = new RegExp(`(?<![\\w.$])${name}\\(`, "g");
  for (const m of src.matchAll(re)) {
    const start = (m.index ?? 0) + m[0].length;
    /** @type {string[]} */
    const args = [];
    let depth = 0;
    let cur = "";
    /**
     * Quote stack: a string's quote, or the depth a template's `${` opened at.
     * @type {(string | number)[]}
     */
    const q = [];
    let i = start;
    for (; i < src.length; i += 1) {
      const c = src[i];
      const top = q.at(-1);
      if (typeof top === "string") {
        cur += c;
        if (c === "\\") {
          cur += src[i + 1] ?? "";
          i += 1;
        } else if (c === top) q.pop();
        else if (top === "`" && c === "$" && src[i + 1] === "{") {
          cur += "{";
          i += 1;
          q.push(depth);
        }
        continue;
      }
      if (c === '"' || c === "'" || c === "`") {
        q.push(c);
        cur += c;
        continue;
      }
      if (c === "/" && src[i + 1] === "/") {
        while (i < src.length && src[i] !== "\n") i += 1;
        continue;
      }
      if (c === "/" && src[i + 1] === "*") {
        i = src.indexOf("*/", i + 2) + 1;
        continue;
      }
      if (typeof top === "number" && c === "}" && depth === top) {
        q.pop();
        cur += c;
        continue;
      }
      if ("([{".includes(c)) depth += 1;
      if (")]}".includes(c)) {
        if (depth === 0) break;
        depth -= 1;
      }
      if (c === "," && depth === 0 && !q.length) {
        args.push(cur.trim());
        cur = "";
        continue;
      }
      cur += c;
    }
    if (cur.trim()) args.push(cur.trim());
    out.push({
      at: src.slice(0, m.index).split("\n").length,
      // `function name(…)`: the definition, not a call.
      def: /function\s+$/.test(
        src.slice(Math.max(0, (m.index ?? 0) - 12), m.index),
      ),
      args,
    });
  }
  return out;
}

/** The constants a module binds to `fn(…)` (`const X = declare({…})`). */
const boundTo = (/** @type {string} */ src, /** @type {string} */ fn) =>
  new Set(
    [...src.matchAll(new RegExp(`const (\\w+) = ${fn}\\(`, "g"))].map(
      (m) => m[1],
    ),
  );

/**
 * What a module does wrong marking elements for Live view: a hand-written
 * mark, or a control id that is not a declared one.
 * @param {string} file
 * @param {string} src
 * @param {Set<string>} declared every declared control id
 */
export function markFaults(file, src, declared) {
  /** @type {string[]} */
  const bad = [];
  // A hand-written mark skips the declaration the registry needs: the
  // dataset, the attribute, an object of attributes, or HTML text.
  for (const m of src.matchAll(
    /dataset\.drive(?:Row)?\s*=(?!=)|["']data-drive(?:-row)?["']\s*:|setAttribute\(\s*["'`]data-drive(?:-row)?["'`]|<[a-z][^<>]*\sdata-drive(?:-row)?=/g,
  ))
    bad.push(`${file}: writes ${m[0]} by hand (use drivable())`);
  const consts = boundTo(src, "declare");
  // A member read (`action.drive`) is fine when every `drive:` the module
  // writes is a declared constant.
  const driveProps = [...src.matchAll(/\bdrive:\s*([^,}\n]+)/g)].map((m) =>
    m[1].trim(),
  );
  for (const { at, args } of calls(src, "drivable")) {
    const id = args[1] ?? "";
    const lit = /^["']([^"']+)["']$/.exec(id);
    if (lit) {
      if (!declared.has(lit[1]))
        bad.push(`${file}:${at}: drivable(…, "${lit[1]}") was never declared`);
    } else if (/^[A-Z][A-Z0-9_]*$/.test(id)) {
      if (!consts.has(id))
        bad.push(
          `${file}:${at}: drivable(…, ${id}): ${id} is not a declare() constant of this module`,
        );
    } else if (/^[^?]+\?\s*[A-Z][A-Z0-9_]*\s*:\s*[A-Z][A-Z0-9_]*$/.test(id)) {
      // One of two declared constants.
      const [, a, b] =
        /\?\s*([A-Z][A-Z0-9_]*)\s*:\s*([A-Z][A-Z0-9_]*)$/.exec(id) ?? [];
      for (const x of [a, b])
        if (!consts.has(x))
          bad.push(
            `${file}:${at}: drivable(…, ${id}): ${x} is not a declare() constant of this module`,
          );
    } else if (/^\w+\.drive$/.test(id)) {
      const wrong = driveProps.filter((p) => !consts.has(p));
      if (wrong.length)
        bad.push(
          `${file}:${at}: drivable(…, ${id}) where a drive: is ${wrong.join(", ")}, not a declare() constant`,
        );
    } else if (/^\w+\.id$/.test(id) && file === "ui.js") {
      // The shared kit marks what its caller names: a `Drive` the caller
      // passes as `drive: { id: X }`, checked at the caller below.
    } else
      bad.push(
        `${file}:${at}: drivable(…, ${id}) computes its id (use a declare() constant)`,
      );
  }
  // A caller's `drive: { id: X }` for a ui.js block: X is a declare()
  // constant of the module, or a declared literal.
  for (const m of src.matchAll(/\bdrive:\s*\{\s*id:\s*([^,}\s]+)/g)) {
    const id = m[1];
    const at = src.slice(0, m.index).split("\n").length;
    const lit = /^["']([^"']+)["']$/.exec(id);
    if (id === "string") continue; // a JSDoc type
    if (lit ? !declared.has(lit[1]) : !consts.has(id))
      bad.push(
        `${file}:${at}: drive: { id: ${id} } is not a declare() constant of this module`,
      );
  }
  return bad;
}

test("drive-reach: no page marks an element for Live view except through declare/drivable", () => {
  const declared = new Set(controls().map((c) => c.id));
  /** @type {string[]} */
  const bad = [];
  for (const [file, src] of sources()) {
    if (file === "drivable.js") continue;
    bad.push(...markFaults(file, src, declared));
  }
  assert.deepEqual(bad, [], bad.join("\n"));
});

test("redesign-drive-5: the undeclared-mark check catches every way around drivable()", () => {
  const declared = new Set(["new-schedule"]);
  const cases = {
    attr: `el.setAttribute("data-drive", "x");`,
    html: 'box.innerHTML = `<button data-drive="x">Go</button>`;',
    computed: `drivable(b, \`schedule-\${kind}\`);`,
    passThroughOutsideKit: `drivable(b, d.id);`,
    undeclaredDrive: `toggleChips({ drive: { id: NOPE } });`,
    notConst: `const NOPE = "new-schedule";\ndrivable(b, NOPE);`,
    nested: `drivable(h("button", { a: 1, b: [2, 3] }, f(x, y)), "nope-x");`,
  };
  for (const [name, src] of Object.entries(cases))
    assert.ok(
      markFaults(name, src, declared).length > 0,
      `${name} passed: ${src}`,
    );
  // The real shapes pass.
  const good = `const NEW = declare({ id: "new-schedule" });\ndrivable(h("button", { a: 1, b: [2, 3] }, f(x, y)), NEW, row);\nconst t = { drive: NEW };\ndrivable(b, t.drive);`;
  assert.deepEqual(markFaults("good", good, declared), []);
  assert.deepEqual(
    markFaults(
      "ui.js",
      "const drive = (e, d) => drivable(e, d.id, d.row);",
      declared,
    ),
    [],
  );
  assert.deepEqual(
    markFaults(
      "p",
      `const K = declare({ id: "new-schedule" });\nkpi({ drive: { id: K, row } });`,
      declared,
    ),
    [],
  );
});

/**
 * What a page module does wrong with its fields (review M5): an input,
 * select or textarea with an id that is no declared field.
 * @param {string} file
 * @param {string} src
 * @param {Set<string>} declared every declared field id
 */
export function fieldFaults(file, src, declared) {
  /** @type {string[]} */
  const bad = [];
  const consts = boundTo(src, "declareField");
  const ok = (/** @type {string} */ e) => {
    const lit = /^["']([^"']+)["']$/.exec(e);
    if (lit) return declared.has(lit[1]);
    if (/^fieldId\(\s*[A-Z][A-Z0-9_]*\s*,/.test(e))
      return consts.has(/^fieldId\(\s*(\w+)/.exec(e)?.[1] ?? "");
    return consts.has(e);
  };
  for (const { at, args } of calls(src, "h")) {
    if (!/^["'](input|select|textarea)["']$/.test(args[0] ?? "")) continue;
    const props = args[1] ?? "";
    if (!props.startsWith("{")) continue;
    // The id property, or its shorthand (a parameter or a variable).
    const inner = calls(`f(${props.slice(1, -1)})`, "f")[0]?.args ?? [];
    const idProp = inner.find((a) => /^id\s*(:|$)/.test(a));
    if (!idProp) continue;
    const expr = idProp.includes(":")
      ? idProp.slice(idProp.indexOf(":") + 1).trim()
      : "id";
    if (ok(expr)) continue;
    // A variable: what the module binds it to (`const id = fieldId(…)`).
    const bound = new RegExp(`const ${expr} = ([^;]+);`).exec(src)?.[1];
    if (bound && ok(bound.trim())) continue;
    // A parameter: every call of the function that has it passes a
    // declared field.
    const fn = [
      ...src
        .slice(0, src.split("\n").slice(0, at).join("\n").length)
        .matchAll(/function (\w+)\(([^)]*)\)/g),
    ].at(-1);
    const params = fn?.[2].split(",").map((p) => p.trim().split(/\s|=/)[0]);
    const pos = params?.indexOf(expr) ?? -1;
    if (fn && pos >= 0) {
      const uses = calls(src, fn[1]).filter(
        (c) => !c.def && c.args.length > pos,
      );
      const wrong = uses.filter((c) => !ok(c.args[pos]));
      if (uses.length && !wrong.length) continue;
    }
    bad.push(
      `${file}:${at}: a field with id ${expr} that is no declared field (declareField, fieldId)`,
    );
  }
  return bad;
}

test("redesign-drive-5: every page field with an id is a declared field", () => {
  const declared = new Set(fields().map((f) => f.id));
  /** @type {string[]} */
  const bad = [];
  for (const [file, src] of sources())
    if (file.startsWith("pages/"))
      bad.push(...fieldFaults(file, src, declared));
  assert.deepEqual(bad, [], bad.join("\n"));
  // The check itself catches the shapes it must.
  const decl = new Set(["shell-line"]);
  assert.ok(fieldFaults("x", `h("input", { id: "nope" })`, decl).length);
  assert.ok(fieldFaults("x", 'h("input", { id: `key-${k}` })', decl).length);
  assert.deepEqual(
    fieldFaults(
      "x",
      `const L = declareField({ id: "shell-line" });\nh("input", { class: "a", id: L });\nfunction sw(id) { return h("input", { id }); }\nsw(L);`,
      decl,
    ),
    [],
  );
  assert.ok(
    fieldFaults(
      "x",
      `function sw(id) { return h("input", { id }); }\nsw("zz");`,
      decl,
    ).length,
  );
});

test("redesign-drive-5: the client and the tab name the same closest controls (shared fixture, code points)", () => {
  const fx = JSON.parse(
    readFileSync(
      new URL("./fixtures/drive-closest.json", import.meta.url),
      "utf8",
    ),
  );
  for (const c of fx.cases)
    assert.deepEqual(
      closest(c.name, c.n, fx.controls).map((/** @type {any} */ x) => x.id),
      c.want,
      c.name,
    );
});

test("redesign-drive-5: a control drawn twice is taken only when declared as twins", () => {
  declare({
    id: "test-review-twins",
    page: "test",
    opens: "run",
    what: "drawn twice on purpose",
    twins: true,
  });
  declare({
    id: "test-review-once",
    page: "test",
    opens: "run",
    what: "drawn once by design",
  });
  const two = (/** @type {string} */ id) => [
    { id, row: null, label: "x" },
    { id, row: null, label: "Cancel" },
  ];
  assert.deepEqual(
    pick({ id: "test-review-twins", row: null }, two("test-review-twins")),
    { index: 0 },
  );
  const r = /** @type {any} */ (
    pick({ id: "test-review-once", row: null }, two("test-review-once"))
  );
  assert.match(String(r.why), /2 times/);
  const cat = /** @type {any} */ (built);
  assert.ok(
    cat.controls.some(
      (/** @type {any} */ c) =>
        c.id === "close-secret-change" && c.twins === true,
    ),
    "the change drawer's x and Cancel are declared twins",
  );
});

test("redesign-drive-5: the catalog carries the router's stack tabs, its schema and the tab's step budget", async () => {
  const c = /** @type {any} */ (built);
  const r = await import("../js/router.js");
  assert.equal(c.schema, 1);
  assert.deepEqual(c.stack_tabs, [
    ...r.STACK_TABS.map((/** @type {any} */ t) => t.tab),
    ...r.RETIRED_STACK_TABS,
  ]);
  assert.ok(
    c.tab_budget_ms > 0 && c.tab_budget_ms <= 12000,
    `${c.tab_budget_ms}`,
  );
  // review M8: formspec.json keeps no hand copy of the router's lists.
  const spec = JSON.parse(
    readFileSync(new URL("../js/formspec.json", import.meta.url), "utf8"),
  );
  assert.equal(spec.pages, undefined);
  assert.equal(spec.stack_tabs, undefined);
  assert.ok(
    !c.controls.some((/** @type {any} */ x) => x.id === "secrets-stack"),
  );
});
