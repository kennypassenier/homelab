// redesign-final (coordinator, 2026-10-04): three orphaned demo dashboards
// answered on fixed ports and a whole-screen run tested one of them. The
// run takes a port the OS hands out, refuses a given port something
// already answers on, checks the server is its own and kills it on every
// way out (scripts/invariants-run.sh).
import { test } from "node:test";
import assert from "node:assert/strict";
import { spawnSync } from "node:child_process";
import { createServer } from "node:net";

const ROOT = new URL("../../../", import.meta.url).pathname;

/** @param {Record<string, string>} env */
const run = (env) =>
  spawnSync("bash", ["scripts/invariants-run.sh"], {
    cwd: ROOT,
    env: { ...process.env, INVARIANTS_PORT_CHECK_ONLY: "1", ...env },
    encoding: "utf8",
    timeout: 20000,
  });

test("redesign-final: a whole-screen run refuses a port something already answers on, and takes a free one by itself", async () => {
  const srv = createServer().listen(0, "127.0.0.1");
  await new Promise((r) => srv.once("listening", r));
  const port = String(/** @type {any} */ (srv.address()).port);
  try {
    const taken = run({ INVARIANTS_PORT: port });
    assert.equal(taken.status, 2, `${taken.stdout}\n${taken.stderr}`);
    assert.match(taken.stderr, new RegExp(`port ${port} is already taken`));
  } finally {
    srv.close();
  }
  const free = run({ INVARIANTS_PORT: "" });
  assert.equal(free.status, 0, `${free.stdout}\n${free.stderr}`);
  assert.match(free.stdout, /port \d+ is free/);
});
