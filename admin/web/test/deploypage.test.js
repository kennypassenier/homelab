// ux-deploypage (Kenny, 2026-11-01): Deploy all changes is the page
// /stacks/deploy. Its pure parts: the Compare note and the workstation line.
import { test } from "node:test";
import assert from "node:assert/strict";
import { APPLY_CLI, compareWords } from "../js/pages/deploy.js";

test("ux-deploypage: the Compare note says how many stacks differ, in plain words", () => {
  assert.equal(compareWords(null), "Comparing every stack with its files…");
  assert.equal(compareWords(0), "Every stack matches its files.");
  assert.match(compareWords(1), /^1 stack differs from its files/);
  assert.match(compareWords(3), /^3 stacks differ from their files/);
  assert.doesNotMatch(compareWords(2), /marked in the list/);
});

test("ux-deploypage: the page carries the command that does the same from a workstation", () => {
  assert.equal(APPLY_CLI, "homelab apply --yes");
});
