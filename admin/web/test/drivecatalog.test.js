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
 * The objects a module builds of declared controls only
 * (`const DRIVE = { follow: declare({…}), … }`): name → its keys.
 * @param {string} src
 */
export function declaredMaps(src) {
  /** @type {Map<string, Set<string>>} */
  const out = new Map();
  for (const m of src.matchAll(/const (\w+) = \{/g)) {
    const open = (m.index ?? 0) + m[0].length - 1;
    let depth = 0;
    let end = open;
    for (; end < src.length; end++) {
      if (src[end] === "{") depth++;
      else if (src[end] === "}" && --depth === 0) break;
    }
    const members = calls(`f(${src.slice(open + 1, end)})`, "f")[0]?.args;
    if (!members?.length) continue;
    const keys = members.map((a) => /^(\w+):\s*declare\(/.exec(a)?.[1]);
    if (keys.every(Boolean))
      out.set(m[1], new Set(/** @type {string[]} */ (keys)));
  }
  return out;
}

/**
 * What a module does wrong marking elements for Live view: a hand-written
 * mark, or a control id that is not a declared one.
 * @param {string} file
 * @param {string} src
 * @param {Set<string>} declared every declared control id
 * @param {Set<string>} [exported] the declare() constants other modules
 *   export (`export const HELP_OPEN = declare(…)`), usable where imported
 */
export function markFaults(file, src, declared, exported = new Set()) {
  /** @type {string[]} */
  const bad = [];
  // A hand-written mark skips the declaration the registry needs: the
  // dataset, the attribute, an object of attributes, or HTML text.
  for (const m of src.matchAll(
    /dataset\.drive(?:Row)?\s*=(?!=)|["']data-drive(?:-row)?["']\s*:|setAttribute\(\s*["'`]data-drive(?:-row)?["'`]|<[a-z][^<>]*\sdata-drive(?:-row)?=/g,
  ))
    bad.push(`${file}: writes ${m[0]} by hand (use drivable())`);
  const consts = boundTo(src, "declare");
  // An exported declare() constant this module imports by name.
  for (const m of src.matchAll(/import \{([^}]*)\} from "[^"]+"/g))
    for (const n of m[1].split(",").map((x) => x.trim()))
      if (exported.has(n)) consts.add(n);
  // A module-local wrapper of declare() (`const ctl = (id, what) =>
  // declare({…})`): what it returns is a declared id too.
  for (const m of src.matchAll(
    /(?:const (\w+) = \([^)]*\)\s*=>\s*declare\(|function (\w+)\([^)]*\)\s*\{\s*return declare\()/g,
  ))
    for (const c of boundTo(src, m[1] ?? m[2])) consts.add(c);
  const maps = declaredMaps(src);
  /** An argument that names a declared control. @param {string} a */
  const named = (a) => {
    const lit = /^["']([^"']+)["']$/.exec(a);
    if (lit) return declared.has(lit[1]);
    if (/^[A-Z][A-Z0-9_]*$/.test(a)) return consts.has(a);
    const mem = /^([A-Z][A-Z0-9_]*)\.(\w+)$/.exec(a);
    return !!mem && !!maps.get(mem[1])?.has(mem[2]);
  };
  /**
   * A parameter of the module-local function around line `at` that every
   * call of that function fills with a declared control.
   * @param {string} name @param {number} at
   */
  const passedDeclared = (name, at) => {
    const before = src.split("\n").slice(0, at).join("\n");
    const fns = [
      ...before.matchAll(
        /(?:function (\w+)\(([^)]*)\)|(?:const|let) (\w+) = \(([^)]*)\)\s*=>)/g,
      ),
    ];
    for (const f of fns.reverse()) {
      const fn = f[1] ?? f[3];
      const params = (f[2] ?? f[4])
        .split(",")
        .map((x) => x.trim().split(/\s|=/)[0]);
      const pos = params.indexOf(name);
      if (pos < 0) continue;
      const uses = calls(src, fn).filter(
        (c) => !c.def && c.args.length > pos && c.at > 0,
      );
      // The definition itself matches `fn(` once for arrows; skip it.
      const real = uses.filter((c) => !c.args[pos].startsWith("/**"));
      return (
        real.length > 0 &&
        real.every((c) => named(c.args[pos]) || c.args[pos] === name)
      );
    }
    return false;
  };
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
      // `string`: a JSDoc type (`{drive: string}`), not a value.
      const wrong = driveProps.filter((p) => p !== "string" && !consts.has(p));
      if (wrong.length)
        bad.push(
          `${file}:${at}: drivable(…, ${id}) where a drive: is ${wrong.join(", ")}, not a declare() constant`,
        );
    } else if (/^[A-Z][A-Z0-9_]*\.\w+$/.test(id)) {
      // A member of an object of declared controls (`DRIVE.follow`).
      const [name, key] = id.split(".");
      if (!maps.get(name)?.has(key))
        bad.push(
          `${file}:${at}: drivable(…, ${id}): ${name} is no object of declare() constants with ${key}`,
        );
    } else if (/^[a-z]\w*$/.test(id) && passedDeclared(id, at)) {
      // A helper's parameter that every caller fills with a declared id.
    } else if (/^\w+\.drive(\.\w+|\[0\])$/.test(id)) {
      // A shared component marks a member of the object its caller passes
      // as `drive: NAME`, checked at the caller below.
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
  // A caller's `drive: [NAME, row]` (the stack hub's action buttons).
  for (const m of src.matchAll(/\bdrive:\s*\[\s*([^,\]\s]+)\s*,/g)) {
    const at = src.slice(0, m.index).split("\n").length;
    if (m[1] !== "string" && !named(m[1]))
      bad.push(
        `${file}:${at}: drive: [${m[1]}, …] is not a declare() constant of this module`,
      );
  }
  // A caller's `drive: NAME`: a declare() constant or an object of them.
  for (const m of src.matchAll(/\bdrive:\s*([A-Z][A-Z0-9_]*)\b(?!\.)/g)) {
    const at = src.slice(0, m.index).split("\n").length;
    if (!consts.has(m[1]) && !maps.has(m[1]))
      bad.push(
        `${file}:${at}: drive: ${m[1]} is no declare() constant or object of them`,
      );
  }
  return bad;
}

test("drive-reach: no page marks an element for Live view except through declare/drivable", () => {
  const declared = new Set(controls().map((c) => c.id));
  /** @type {string[]} */
  const bad = [];
  /** @type {Set<string>} */
  const exported = new Set();
  for (const [, src] of sources())
    for (const m of src.matchAll(/export const (\w+) = declare\(/g))
      exported.add(m[1]);
  for (const [file, src] of sources()) {
    if (file === "drivable.js") continue;
    bad.push(...markFaults(file, src, declared, exported));
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
    undeclaredMap: `const D = { a: declare({ id: "new-schedule" }), b: "x" };\ndrivable(e, D.a);`,
    undeclaredMapKey: `const D = { a: declare({ id: "new-schedule" }) };\ndrivable(e, D.z);`,
    undeclaredPassedMap: `const D = { a: "x" };\nmountHostLog(r, { drive: D });`,
    undeclaredParam: `const go = (b, id) => drivable(b, id);\ngo(x, "nope-x");`,
    undeclaredPair: `button(c, "x", { drive: [NOPE, row] });`,
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
      "p",
      `const D = { a: declare({ id: "new-schedule" }) };\ndrivable(e, D.a);\nmount(r, { drive: D });\ndrivable(e, opts.drive.a);`,
      declared,
    ),
    [],
  );
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

// redesign-integrate-8 (coordinator, 2026-10-03): five merge commits went
// green while 53 declared controls could not be reached on screen — the
// checks above see declarations and marks, not where a page draws a
// control (a view, a row, a state). The whole-screen sweep does, and a
// passing sweep stamps every control it pressed
// (test-e2e/sweep-stamp.json, `sweepKey` of each). A control whose entry no
// passing sweep has pressed since it changed is refused at commit, merge
// commits included (.githooks/drivecatalog.sh runs this file).
test("redesign-integrate-8: every catalog control was pressed by a passing Live view sweep since its entry last changed", async () => {
  const { sweepKey, catalogHash } = await import("../test-e2e/sweepkey.js");
  const file = new URL("../test-e2e/sweep-stamp.json", import.meta.url);
  /** @type {any} */
  let stamp = { controls: [] };
  try {
    stamp = JSON.parse(readFileSync(file, "utf8"));
  } catch (e) {
    if (/** @type {any} */ (e).code !== "ENOENT") throw e;
  }
  const keys = stamp.controls ?? [];
  if (keys.length)
    assert.equal(
      catalogHash([...keys].sort()),
      stamp.catalog,
      "sweep-stamp.json's controls do not hash to the catalog it names: it was edited by hand; only a passing sweep writes it",
    );
  const pressed = new Set(keys);
  /** @type {Map<string, string[]>} */
  const byPage = new Map();
  for (const c of /** @type {any} */ (built).controls)
    if (!pressed.has(sweepKey(c)))
      byPage.set(c.page, [...(byPage.get(c.page) ?? []), c.id]);
  assert.deepEqual(
    [...byPage].map(([p, ids]) => `${p}: ${ids.join(", ")}`),
    [],
    "controls no passing sweep has pressed since they changed: run INVARIANTS_ONLY='drive-reach: Live view finds and presses' scripts/invariants-run.sh, which rewrites admin/web/test-e2e/sweep-stamp.json when it passes",
  );
});
