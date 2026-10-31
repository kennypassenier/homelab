// The browser loads kp-themes' components as one bundle at
// `/static/kp/dist/kp-themes.min.js`, which
// chassis-rs serves from memory; no such file exists on disk. Under
// `node --test` that absolute path would not resolve, so this hook maps it
// onto the copy cargo checked out for the chassis-rs commit Cargo.lock pins.
// The type check does the same through jsconfig.json's `paths`.
import { registerHooks } from "node:module";
import {
  existsSync,
  mkdirSync,
  readFileSync,
  readdirSync,
  writeFileSync,
} from "node:fs";
import { tmpdir } from "node:os";
import { homedir } from "node:os";
import { dirname, join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const PREFIX = "/static/kp/";
/** The single bundle the browser loads; under node it is a barrel over the module tree. */
const BUNDLE = "/static/kp/dist/kp-themes.min.js";

/** The chassis-rs commit the workspace's Cargo.lock pins. */
function pinnedRev() {
  const lock = fileURLToPath(
    new URL("../../../../Cargo.lock", import.meta.url),
  );
  const m = readFileSync(lock, "utf8").match(
    /chassis-rs\?tag=[^#"]+#([0-9a-f]{7,40})/,
  );
  if (!m) throw new Error(`no chassis-rs source in ${lock}`);
  return m[1];
}

/** `<checkout>/crates/chassis/static/kp` for that commit, under CARGO_HOME. */
function kpRoot() {
  const rev = pinnedRev();
  const base = join(
    process.env.CARGO_HOME || join(homedir(), ".cargo"),
    "git",
    "checkouts",
  );
  for (const repo of existsSync(base) ? readdirSync(base) : []) {
    if (!repo.startsWith("chassis-rs-")) continue;
    for (const co of readdirSync(join(base, repo))) {
      const dir = join(base, repo, co, "crates", "chassis", "static", "kp");
      if (rev.startsWith(co) && existsSync(dir)) return dir;
    }
  }
  throw new Error(
    `chassis-rs ${rev} is not checked out under ${base}; run \`cargo fetch\` in the workspace first`,
  );
}

let root;
let barrel;

/**
 * The names a module exports, read from its source: declarations,
 * `export { … }` lists (with or without `from`) and `export * from` chains.
 */
function exportedNames(file, seen = new Set()) {
  if (seen.has(file) || !existsSync(file)) return new Set();
  seen.add(file);
  const text = readFileSync(file, "utf8");
  const names = new Set();
  for (const m of text.matchAll(
    /^export\s+(?:async\s+)?(?:const|let|var|function\*?|class)\s+([A-Za-z0-9_$]+)/gm,
  ))
    names.add(m[1]);
  for (const m of text.matchAll(/^export\s*\{([^}]*)\}/gm))
    for (const part of m[1].split(",")) {
      const name = part
        .trim()
        .split(/\s+as\s+/)
        .pop()
        ?.trim();
      if (name) names.add(name);
    }
  for (const m of text.matchAll(/^export\s*\*\s*from\s*["']([^"']+)["']/gm))
    for (const n of exportedNames(join(dirname(file), m[1]), seen))
      names.add(n);
  return names;
}

/**
 * A file that re-exports every kp-themes module, standing in for the bundle
 * (same names, no browser needed). The module list is the type barrel's, so
 * the two cannot disagree. A name two modules both export (the bundle picks
 * one) is taken from the first module, so no star export is ambiguous.
 */
function bundleBarrel() {
  if (barrel) return barrel;
  const types = fileURLToPath(new URL("../../types/kp", import.meta.url));
  const taken = new Set();
  const lines = [];
  for (const f of readdirSync(types)
    .filter((f) => f.endsWith(".d.ts"))
    .sort()) {
    const name = f.slice(0, -".d.ts".length);
    const file = join(root, "js", `${name}.js`);
    if (!existsSync(file)) continue;
    const fresh = [...exportedNames(file)].filter((n) => !taken.has(n));
    for (const n of fresh) taken.add(n);
    if (fresh.length)
      lines.push(
        `export { ${fresh.join(", ")} } from ${JSON.stringify(pathToFileURL(file).href)};`,
      );
  }
  const dir = join(tmpdir(), "homelab-kp-barrel");
  mkdirSync(dir, { recursive: true });
  barrel = join(dir, `kp-bundle-${process.pid}.mjs`);
  writeFileSync(barrel, `${lines.join("\n")}\n`);
  return barrel;
}

function resolve(specifier, context, next) {
  // drive-reach: the kit's passkey helper (pages/passkeys.js imports it)
  // wires itself to `document` as it loads; under node a stand-in with the
  // one export the page uses, so the control catalog can load every page.
  if (specifier === "/static/passkeys.js")
    return {
      url: "data:text/javascript,export%20const%20registerPasskey%20%3D%20async%20()%20%3D%3E%20%7B%7D%3B",
      shortCircuit: true,
    };
  if (specifier === BUNDLE) {
    root ??= kpRoot();
    return { url: pathToFileURL(bundleBarrel()).href, shortCircuit: true };
  }
  if (specifier.startsWith(PREFIX)) {
    root ??= kpRoot();
    return {
      url: pathToFileURL(join(root, specifier.slice(PREFIX.length))).href,
      shortCircuit: true,
    };
  }
  return next(specifier, context);
}

registerHooks({ resolve });
