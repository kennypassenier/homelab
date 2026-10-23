// update-nothing-1 and native-cadvisor-1 (the Update flow's half), Kenny,
// 2026-10-23: the flow offers an update only when there is something to
// update. A stack with every image pinned by digest and none newer shows
// "nothing to update" and no ticked row; the pull row exists only for an
// app on a moving tag; a native-only stack has none.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  firstChosen,
  isMovingTag,
  itemsFor,
  movingApps,
  splitByPullState,
} from "../js/updateflow.js";

const GATEWAY = {
  "crowdsec/crowdsec": `crowdsecurity/crowdsec:v1.8.1@sha256:${"a".repeat(64)}`,
  "socket-proxy/socket-proxy": `tecnativa/docker-socket-proxy:v0.5.0@sha256:${"b".repeat(64)}`,
  "cloudflared/cloudflared": `cloudflare/cloudflared:2026.10.0@sha256:${"c".repeat(64)}`,
  "traefik/traefik": `traefik:v3.7.16@sha256:${"d".repeat(64)}`,
};
const NOTHING_NEWER = { measured_at: 1, images: [] };
const stack = { all: /** @type {const} */ (false), stack: "gateway" };

test("update-nothing-1: an image pinned by digest is no moving tag; a bare name or :latest is", () => {
  assert.equal(isMovingTag(GATEWAY["traefik/traefik"]), false);
  assert.equal(isMovingTag("jellyfin/jellyfin:latest"), true);
  assert.equal(isMovingTag("nginx"), true);
  assert.equal(isMovingTag("nginx:1.27"), true);
  // The files were not read: no claim either way.
  assert.equal(isMovingTag(""), false);
  assert.equal(isMovingTag(null), false);
});

test("update-nothing-1: a fully pinned, current stack offers no Update", () => {
  assert.deepEqual(movingApps(GATEWAY), []);
  const items = itemsFor(NOTHING_NEWER, stack, movingApps(GATEWAY));
  assert.deepEqual(items, [], "nothing to update is nothing offered");
  assert.equal(firstChosen(items, stack).size, 0, "nothing starts ticked");
});

test("update-nothing-1: the pull row exists for a stack with an app on a moving tag, once per app", () => {
  const images = {
    ...GATEWAY,
    "whoami/whoami": "traefik/whoami:latest",
    "whoami/sidecar": "busybox",
  };
  assert.deepEqual(movingApps(images), ["whoami"]);
  const items = itemsFor(NOTHING_NEWER, stack, movingApps(images));
  assert.deepEqual(
    items.map((i) => i.id),
    ["pull:gateway"],
  );
  assert.deepEqual([...firstChosen(items, stack)], ["pull:gateway"]);
  // An opener that picked one app: only when that app is on a moving tag.
  const one = { ...stack, app: "whoami" };
  assert.deepEqual(
    itemsFor(NOTHING_NEWER, one, movingApps(images)).map((i) => i.id),
    ["pull:gateway"],
  );
  assert.deepEqual(
    itemsFor(NOTHING_NEWER, { ...stack, app: "traefik" }, movingApps(images)),
    [],
    "a pinned app has no pull",
  );
});

test("native-cadvisor-1: a native-only stack has no compose app on a moving tag, so no pull row sends an app that is not part of the stack", () => {
  // admin and kitchen: no docker-compose.yml, so the files name no image.
  assert.deepEqual(movingApps({}), []);
  assert.deepEqual(movingApps(null), []);
  assert.deepEqual(
    itemsFor(NOTHING_NEWER, { all: false, stack: "admin" }, movingApps({})),
    [],
  );
});

test("update-nothing-1: a pinned move still lists beside it; only the pull row depends on the files", () => {
  const stale = {
    measured_at: 1,
    images: [
      {
        where_: "gateway/traefik",
        pinned: "v3.7.16",
        latest: "v3.8.0",
        upstream: "github.com/traefik/traefik",
        key: "traefik/traefik",
      },
    ],
  };
  const items = itemsFor(stale, stack, movingApps(GATEWAY));
  assert.deepEqual(
    items.map((i) => i.id),
    ["pin:gateway:traefik/traefik"],
  );
  // The files could not be read (null): the old offer stays, a pull row too.
  assert.deepEqual(
    itemsFor(NOTHING_NEWER, stack, null).map((i) => i.id),
    ["pull:gateway"],
  );
});

test("update-nothing-1: only an app measured current leaves the pull row", () => {
  const r = splitByPullState(["a", "b", "c"], { a: "current", b: "newer" });
  assert.deepEqual(r.current, ["a"]);
  assert.deepEqual(r.pullable, ["b", "c"], "newer and unmeasured stay");
  assert.deepEqual(splitByPullState(["a"], null).pullable, ["a"]);
});
