// The browser loads kp-themes' components from `/static/kp/js/*`, which
// chassis-rs serves from memory; no such file exists on disk. Under
// `node --test` that absolute path would not resolve, so this hook maps it
// onto the copy cargo checked out for the chassis-rs commit Cargo.lock pins.
// The type check does the same through jsconfig.json's `paths`.
import { registerHooks } from "node:module";
import { existsSync, readFileSync, readdirSync } from "node:fs";
import { homedir } from "node:os";
import { join } from "node:path";
import { fileURLToPath, pathToFileURL } from "node:url";

const PREFIX = "/static/kp/";

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

function resolve(specifier, context, next) {
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
