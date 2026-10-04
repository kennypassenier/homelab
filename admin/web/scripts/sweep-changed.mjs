// redesign-final (coordinator, 2026-10-04): the Live view controls this tree
// changed against HEAD (added, moved, given a `was`), as the regular
// expression INVARIANTS_SWEEP_ONLY takes; nothing printed when none did.
// scripts/sweep-changed.sh runs the sweep of just those.
//   node --import ./test/support/kp-register.mjs scripts/sweep-changed.mjs
import { execFileSync } from "node:child_process";
import { buildCatalog } from "./drivecatalog.mjs";
import { changedControls } from "../test-e2e/sweepkey.js";

/** @type {any[] | null} */
let head = null;
try {
  head = JSON.parse(
    execFileSync("git", ["show", "HEAD:admin/web/js/drivecatalog.json"], {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
      maxBuffer: 64 * 1024 * 1024,
    }),
  ).controls;
} catch {
  head = null;
}
const now = /** @type {any} */ (await buildCatalog()).controls;
const ids = changedControls(head, now);
if (ids.length) {
  console.error(`changed against HEAD: ${ids.join(", ")}`);
  console.log(`^(${ids.map((i) => i.replace(/[-]/g, "\\-")).join("|")})$`);
}
