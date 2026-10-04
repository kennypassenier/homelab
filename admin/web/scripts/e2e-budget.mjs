// redesign-final-51: the whole-screen suite's budget (test-e2e/budget.js).
//   node scripts/e2e-budget.mjs run <tap file> <seconds>
//       a full run against the budget: prints it, exits 1 when over
//   node scripts/e2e-budget.mjs commit
//       the staged budget.json against HEAD's: a raise needs a new reason
import { readFileSync } from "node:fs";
import { execFileSync } from "node:child_process";
import {
  measureOf,
  overBudget,
  raiseFaults,
  readBudget,
} from "../test-e2e/budget.js";

const [mode, ...args] = process.argv.slice(2);
if (mode === "run") {
  const b = readBudget();
  const run = measureOf(readFileSync(args[0], "utf8"), Number(args[1]));
  console.log(
    `invariants: budget: ${run.seconds} s of ${b.seconds} s, ${run.cases} cases of ${b.cases} (fails above +20 %; admin/web/test-e2e/budget.json)`,
  );
  const over = overBudget(b, run);
  for (const o of over) console.error(`invariants: OVER BUDGET — ${o}`);
  if (over.length) {
    console.error(
      "Remedy: prune the cases that grew it, or raise the budget in a commit that edits admin/web/test-e2e/budget.json with a new reason line.",
    );
    process.exit(1);
  }
} else if (mode === "commit") {
  const git = (/** @type {string[]} */ a) =>
    execFileSync("git", a, {
      encoding: "utf8",
      stdio: ["ignore", "pipe", "ignore"],
    });
  const path = "admin/web/test-e2e/budget.json";
  const after = JSON.parse(git(["show", `:${path}`]));
  let before = null;
  try {
    before = JSON.parse(git(["show", `HEAD:${path}`]));
  } catch {
    before = null;
  }
  const f = raiseFaults(before, after);
  for (const x of f) console.error(x);
  if (f.length) process.exit(1);
} else {
  console.error("usage: e2e-budget.mjs run <tap> <seconds> | commit");
  process.exit(2);
}
