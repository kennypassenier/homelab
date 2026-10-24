// feat-kp11demo-14: what the connection banner shows for what the live
// channel and the host link say.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  connectionFor,
  gatewayJob,
  offlineWords,
} from "../js/connectionbar.js";

const base = { up: true, link: "host 3.75.0", queued: 0 };

test("an open channel with the host link up is online; a connecting one shows nothing", () => {
  assert.equal(
    connectionFor({ ...base, stream: { state: "open", attempt: 0 } }).state,
    "online",
  );
  assert.equal(
    connectionFor({ ...base, stream: { state: "connecting", attempt: 0 } })
      .state,
    "online",
  );
});

test("a dropped channel is offline, a new one reconnecting with its attempt, and queued jobs are counted exactly", () => {
  const down = connectionFor({
    ...base,
    queued: 3,
    stream: { state: "down", attempt: 2 },
  });
  assert.equal(down.state, "offline");
  assert.equal(down.detail.pending, 3);
  const trying = connectionFor({
    ...base,
    stream: { state: "trying", attempt: 2 },
  });
  assert.equal(trying.state, "reconnecting");
  assert.equal(trying.detail.attempt, 2);
});

test("an open channel while the host link is down is refused, with the host's own words", () => {
  const c = connectionFor({
    up: false,
    link: "host link down: connection refused",
    queued: 0,
    stream: { state: "open", attempt: 0 },
  });
  assert.equal(c.state, "refused");
  assert.equal(c.detail.why, "host link down: connection refused");
});

// offline-flash-1 (Kenny, 2026-10-24): the gateway carries the dashboard's own
// connection, so a job on it is the likely reason the live channel drops.
test("a running job of the gateway stack is named; other jobs and finished ones are not", () => {
  assert.equal(
    gatewayJob([
      { stack: "films", action: "deploy", state: "running" },
      { stack: "gateway", action: "backup", state: "running" },
    ]),
    "backup",
  );
  assert.equal(
    gatewayJob([{ stack: "gateway", action: "deploy", state: "done" }]),
    null,
  );
  assert.equal(
    gatewayJob([{ stack: "films", action: "deploy", state: "running" }]),
    null,
  );
});

test("the offline words say the gateway is being backed up or deployed when its job runs", () => {
  assert.equal(offlineWords(null).title, "You are offline");
  const backup = offlineWords("backup");
  assert.match(backup.title, /gateway is being backed up/);
  assert.match(backup.why, /reconnects by itself/);
  assert.match(offlineWords("deploy").title, /gateway is being deployed/);
});
