// redesign-integrate-4 (3.71.0 consolidation): the Roll back… dialog runs
// on the dashboard's server as ONE update-apps job, like the Update flow
// (review-flows finding 3's fault class): a closed tab never stops it
// half-way between its backup, its commit and its deploy.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";

test("redesign-integrate-4: a roll back is one update-apps job, from the image it runs back to the earlier one", async () => {
  const pin = /** @type {any} */ (await import("../js/pinupdate.js"));
  assert.equal(
    typeof pin.pinJobBody,
    "function",
    "pinupdate.js has no pinJobBody: the roll back still runs from the tab",
  );
  // The move as the dialog shows a roll back: from the image it runs now
  // to the one it ran before the update.
  const back = {
    stack: "beta-demo",
    key: "api/api",
    file: "stacks/beta-demo/apps/api/docker-compose.yml",
    from: "ghcr.io/x/api:v3.0.0@sha256:bbb",
    to: "ghcr.io/x/api:v2.3.0@sha256:aaa",
    from_version: "v3.0.0",
    to_version: "v2.3.0",
  };
  const body = pin.pinJobBody(back);
  assert.deepEqual(Object.keys(body), ["updates"]);
  assert.deepEqual(JSON.parse(body.updates), [
    {
      stack: "beta-demo",
      kind: "pin",
      key: "api/api",
      app: "api",
      from: back.from,
      to: back.to,
    },
  ]);
  // The dialog's roll back posts that job and nothing of its own.
  const src = readFileSync(
    new URL("../js/pinupdate.js", import.meta.url),
    "utf8",
  );
  const i = src.indexOf("async function runOnServer(");
  assert.ok(i >= 0, "no server-side run for the roll back");
  const fn = src.slice(i, src.indexOf("\n}\n", i));
  assert.match(fn, /\/data\/actions\/_host\/update-apps/);
  assert.doesNotMatch(fn, /\/backup`|\/commit`/);
  assert.match(
    src,
    /o\.rollback\s*\?\s*runOnServer\(/,
    "the confirm of a roll back does not go to the server job",
  );
});
