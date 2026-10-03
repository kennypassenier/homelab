// redesign-final-12 (coordinator, 2026-10-04): four whole-screen cases were
// red on redesign-371 although every branch had passed its own cases —
// nothing ran the cases a MERGE affects. This maps the admin/web files a
// merge changes to the whole-screen cases that open the pages built from
// them, from the code itself (no hand-kept list):
//   - main.js: which page module draws which route page (`case "x":` →
//     `cleanup = fn(` → `import { mount as fn } from "./pages/F.js"`);
//   - each page module's static import closure, and the page stylesheets
//     its closure loads (`ensureStyle("/css/pages/F.css")`); a stylesheet
//     index.html links, and a file every page imports, is shared;
//   - each case's addresses (`${BASE}/…` and quoted "/…" paths in its
//     body), resolved to a route page by the router itself.
// A case that names no address (it walks every page) runs when a shared
// file changed. `.githooks/merge-cases.sh` runs what this prints.
//
// Usage: node scripts/merge-cases.mjs <changed file>... (paths from the
// repository root or admin/web); prints {pages, cases, pattern} as JSON.
import { readFileSync, readdirSync, existsSync } from "node:fs";
import { dirname, join, normalize, relative } from "node:path";
import { fileURLToPath } from "node:url";

const WEB = join(dirname(fileURLToPath(import.meta.url)), "..");

/**
 * Route page → page module (admin/web-relative), read off main.js.
 * @param {string} main main.js's source
 * @returns {Map<string, string>}
 */
export function pageModules(main) {
  /** @type {Map<string, string>} */
  const fnFile = new Map();
  for (const m of main.matchAll(
    /import\s*\{\s*mount as (\w+)\s*\}\s*from\s*"\.\/(pages\/[\w-]+\.js)"/g,
  ))
    fnFile.set(m[1], `js/${m[2]}`);
  /** @type {Map<string, string>} */
  const out = new Map();
  for (const m of main.matchAll(/case "([\w-]+)":\s*\n\s*cleanup = (\w+)\(/g)) {
    const f = fnFile.get(m[2]);
    if (f) out.set(m[1], f);
  }
  // A view of a page (`VIEW_MOUNTS`): `id: (root) => fn(`.
  for (const m of main.matchAll(/^\s*(\w+): \(root\) => (\w+)\(/gm)) {
    const f = fnFile.get(m[2]);
    if (f && !out.has(m[1])) out.set(m[1], f);
  }
  return out;
}

/**
 * Every admin/web file a module pulls in: its static imports (recursively)
 * and the page stylesheets they load.
 * @param {string} file admin/web-relative
 * @param {(f: string) => string | null} read
 * @returns {Set<string>}
 */
export function closure(file, read) {
  /** @type {Set<string>} */
  const seen = new Set();
  const walk = (/** @type {string} */ f) => {
    if (seen.has(f)) return;
    const src = read(f);
    if (src == null) return;
    seen.add(f);
    for (const m of src.matchAll(
      /(?:import|export)[^;]*?from\s*"(\.{1,2}\/[^"]+\.js)"/g,
    ))
      walk(normalize(join(dirname(f), m[1])));
    for (const m of src.matchAll(/ensureStyle\(\s*"\/(css\/[^"]+\.css)"\s*\)/g))
      seen.add(m[1]);
  };
  walk(file);
  return seen;
}

/**
 * The cases of a whole-screen suite and the addresses each opens. A case
 * whose name is a template (`${…}`) is left out: it is one of a family the
 * gate runs whole.
 * @param {string} src a *.e2e.js file
 * @returns {{name: string, paths: string[]}[]}
 */
export function casesOf(src) {
  const starts = [...src.matchAll(/^test\(\s*\n?\s*(["`])((?:(?!\1).)+)\1/gm)];
  return starts
    .map((m, i) => {
      const body = src.slice(
        m.index ?? 0,
        i + 1 < starts.length ? starts[i + 1].index : src.length,
      );
      const paths = new Set();
      for (const p of body.matchAll(/\$\{BASE\}(\/[^`$\s]*)/g)) paths.add(p[1]);
      for (const p of body.matchAll(/["'](\/[a-z][\w/?=&.,%-]*)["']/g))
        if (!p[1].startsWith("/data/") && !p[1].startsWith("/static/"))
          paths.add(p[1]);
      // An address built at run time (`${BASE}/${path}` over a list of
      // pages) walks every page: a case for any shared change.
      if (/\$\{BASE\}\/?\$\{/.test(body)) return { name: m[2], paths: [] };
      return { name: m[2], paths: [...paths] };
    })
    .filter((c) => !c.name.includes("${"));
}

/** @param {string} s */
const escape = (s) => s.replace(/[.*+?^${}()|[\]\\]/g, "\\$&");

/**
 * The cases a change set affects.
 * @param {string[]} changed admin/web-relative paths
 * @param {{modules: Map<string, string>, closures: Map<string, Set<string>>,
 *   shared: Set<string>, cases: {name: string, paths: string[]}[],
 *   pageOf: (path: string) => string | null}} x
 */
export function affected(changed, x) {
  const sharedHit = changed.some((f) => x.shared.has(f));
  /** @type {Set<string>} */
  const pages = new Set();
  for (const [page, file] of x.modules)
    if (changed.some((f) => x.closures.get(file)?.has(f))) pages.add(page);
  const cases = x.cases.filter((c) => {
    if (!c.paths.length) return sharedHit;
    return c.paths.some((p) => {
      const page = x.pageOf(p);
      return page != null && pages.has(page);
    });
  });
  const names = cases.map((c) => c.name);
  return {
    pages: [...pages].sort(),
    cases: names,
    pattern: names.length ? `^(?:${names.map(escape).join("|")})$` : "",
  };
}

/**
 * The affected pages and cases of a change set, read from this tree.
 * @param {string[]} changedPaths from the repository root or admin/web
 */
export async function plan(changedPaths) {
  const read = (/** @type {string} */ f) => {
    const p = join(WEB, f);
    return existsSync(p) ? readFileSync(p, "utf8") : null;
  };
  const { route, redirectFor } = await import("../js/router.js");
  const modules = pageModules(/** @type {string} */ (read("js/main.js")));
  /** @type {Map<string, Set<string>>} */
  const closures = new Map();
  for (const f of new Set(modules.values())) closures.set(f, closure(f, read));
  // Shared: a stylesheet every page gets (index.html), or a file in the
  // closure of at least half the page modules (ui.js, dom.js, …).
  const html = read("index.html") ?? "";
  const shared = new Set(
    [...html.matchAll(/href="\/(css\/[^"]+\.css)"/g)].map((m) => m[1]),
  );
  const all = [...closures.values()];
  /** @type {Map<string, number>} */
  const uses = new Map();
  for (const c of all) for (const f of c) uses.set(f, (uses.get(f) ?? 0) + 1);
  for (const [f, n] of uses) if (n * 2 >= all.length) shared.add(f);
  for (const f of [
    "js/main.js",
    "js/chrome.js",
    "js/router.js",
    "js/areas.js",
    "index.html",
  ])
    shared.add(f);
  const cases = readdirSync(join(WEB, "test-e2e"))
    .filter((f) => f.endsWith(".e2e.js"))
    .flatMap((f) => casesOf(/** @type {string} */ (read(`test-e2e/${f}`))));
  const pageOf = (/** @type {string} */ path) => {
    const [p, q = ""] = path.split("?");
    let r = route(p);
    if (r.page === "retired") {
      const to = redirectFor(r, q ? `?${q}` : "", { stacks: ["kp-soft"] });
      if (!to) return null;
      r = route(to.split("?")[0]);
    }
    return r.page === "notfound" ? null : r.page;
  };
  const changed = changedPaths
    .map((f) => (f.startsWith("admin/web/") ? f.slice("admin/web/".length) : f))
    .map((f) => relative(WEB, join(WEB, f)));
  return affected(changed, { modules, closures, shared, cases, pageOf });
}

const main = async () => {
  const out = await plan(process.argv.slice(2));
  process.stdout.write(`${JSON.stringify(out)}\n`);
};

if (
  process.argv[1] &&
  fileURLToPath(import.meta.url) === normalize(process.argv[1])
)
  await main();
