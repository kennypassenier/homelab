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

/** Name -> the module file that exports it; the first module wins a name two share (as the bundle picks one). */
function nameIndex() {
  const types = fileURLToPath(new URL("../../types/kp", import.meta.url));
  const index = new Map();
  // `auto.js` re-exports what the others define and starts the page on
  // load, so it is read last: a name goes to the module that defines it.
  const byName = (a, b) =>
    (a === "auto.d.ts") - (b === "auto.d.ts") || (a < b ? -1 : 1);
  for (const f of readdirSync(types)
    .filter((f) => f.endsWith(".d.ts"))
    .sort(byName)) {
    const file = join(root, "js", `${f.slice(0, -".d.ts".length)}.js`);
    // `auto.js` starts the page on load and needs a real DOM (kp-themes 12
    // reads `MutationObserver` off the window); a test gets a no-op
    // `attachAll` below instead, and never attaches a page.
    if (!existsSync(file) || file.endsWith("/auto.js")) continue;
    for (const n of exportedNames(file)) if (!index.has(n)) index.set(n, file);
  }
  return index;
}

let index;
const barrels = new Map();

/** The names `parent` imports from the bundle, or null when it imports it whole (`* as`, dynamic). */
function wantedNames(parent) {
  let text;
  try {
    text = readFileSync(fileURLToPath(parent), "utf8");
  } catch {
    return null;
  }
  const names = new Set();
  const from = BUNDLE.replace(/[.*+?^${}()|[\]\\/]/g, "\\$&");
  const named = new RegExp(
    `import\\s*\\{([^}]*)\\}\\s*from\\s*["']${from}["']`,
    "g",
  );
  for (const m of text.matchAll(named))
    for (const part of m[1].split(","))
      if (part.trim())
        names.add(
          part
            .trim()
            .split(/\s+as\s+/)[0]
            .trim(),
        );
  const whole = new RegExp(`import\\s*\\*\\s*as|import\\(\\s*["']${from}`);
  return whole.test(text) ? null : names;
}

/**
 * A stand-in for the bundle: re-exports, from the module that has each one,
 * only the names `parent` imports, so a test loads just the modules the
 * browser's bundle would hand that page (the whole tree would also start
 * `auto.js`, which needs a page). A parent that imports it whole gets every
 * name.
 */
function bundleBarrel(parent) {
  index ??= nameIndex();
  const wanted = parent ? wantedNames(parent) : null;
  const names = [...(wanted ?? index.keys())]
    .filter((n) => index.has(n))
    .sort();
  const key = names.join(",");
  if (barrels.has(key)) return barrels.get(key);
  const byFile = new Map();
  for (const n of names) {
    const f = index.get(n);
    byFile.set(f, [...(byFile.get(f) ?? []), n]);
  }
  const lines = [];
  if (wanted?.has("attachAll") || (!wanted && names.length))
    lines.push("export const attachAll = () => [];");
  lines.push(
    ...[...byFile].map(
      ([f, ns]) =>
        `export { ${ns.join(", ")} } from ${JSON.stringify(pathToFileURL(f).href)};`,
    ),
  );
  const dir = join(tmpdir(), "homelab-kp-barrel");
  mkdirSync(dir, { recursive: true });
  const file = join(dir, `kp-bundle-${process.pid}-${barrels.size}.mjs`);
  writeFileSync(file, `${lines.join("\n")}\n`);
  barrels.set(key, file);
  return file;
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
    return {
      url: pathToFileURL(bundleBarrel(context.parentURL)).href,
      shortCircuit: true,
    };
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
