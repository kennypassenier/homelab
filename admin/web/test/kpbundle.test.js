// Every name the dashboard imports from kp-themes' single bundle is a name the
// bundle really exports. A bundler drops a name two modules both export (setProgress,
// OPEN_EVENT were such), the type barrel and the node stand-in do not notice, and the
// first sign is a page that never draws in the browser. This reads the pinned bundle's
// own export list, so the check holds against what the server will send.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join } from "node:path";
import { fileURLToPath } from "node:url";

const BUNDLE = "/static/kp/dist/kp-themes.min.js";
const web = fileURLToPath(new URL("..", import.meta.url));

/** @param {string} dir @param {string[]} out */
function sources(dir, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "types") continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) sources(p, out);
    else if (p.endsWith(".js")) out.push(p);
  }
  return out;
}

/** The bundle's export names, read from its `export{…}` lists. */
function bundleExports() {
  const css = fileURLToPath(
    import.meta.resolve("/static/kp/dist/kp-themes.css"),
  );
  const text = readFileSync(css.replace(/\.css$/, ".min.js"), "utf8");
  const names = new Set();
  for (const m of text.matchAll(/export\s*\{([^}]*)\}/g))
    for (const part of m[1].split(",")) {
      const name = part
        .trim()
        .split(/\s+as\s+/)
        .pop()
        ?.trim();
      if (name) names.add(name);
    }
  return names;
}

test("every name imported from the kp-themes bundle is exported by it", () => {
  const exported = bundleExports();
  assert.ok(exported.size > 1000, `only ${exported.size} exports read`);
  const missing = [];
  for (const file of sources(web)) {
    const text = readFileSync(file, "utf8");
    for (const m of text.matchAll(
      new RegExp(
        `import\\s*\\{([^}]*)\\}\\s*from\\s*["']${BUNDLE.replace(/[.]/g, "\\.")}["']`,
        "g",
      ),
    ))
      for (const part of m[1].split(",")) {
        const name = part
          .trim()
          .split(/\s+as\s+/)[0]
          .trim();
        if (name && !exported.has(name))
          missing.push(`${file.slice(web.length)}: ${name}`);
      }
  }
  assert.deepEqual(missing, [], "the bundle does not export these");
});
