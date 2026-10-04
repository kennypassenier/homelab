// redesign-final (coordinator, 2026-10-04): the release gate's demand on the
// Live view sweep — a FULL stamp (every control of this very catalog
// pressed by one passing fixed-order sweep). A commit may carry a partial
// stamp (the controls it changed); the gate never accepts one. Exit 0 and
// silence when it holds, else exit 1 with why.
//   node --import ./test/support/kp-register.mjs scripts/sweep-gate.mjs
import { readFileSync } from "node:fs";
import { buildCatalog } from "./drivecatalog.mjs";
import { gateRefusal } from "../test-e2e/sweepkey.js";

/** @type {any} */
let stamp = null;
try {
  stamp = JSON.parse(
    readFileSync(
      new URL("../test-e2e/sweep-stamp.json", import.meta.url),
      "utf8",
    ),
  );
} catch {
  stamp = null;
}
const why = gateRefusal(
  stamp,
  /** @type {any} */ (await buildCatalog()).controls,
);
if (why) {
  console.error(`sweep-gate: ${why}`);
  process.exit(1);
}
