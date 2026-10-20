// redesign-stacks (3.71.0, the approved Apps demo): the Apps page's view
// model — a tile's dot from the minute watch, the launch search, the board
// with Starred on top, and the verdict line.
import { test } from "node:test";
import assert from "node:assert/strict";
import { appsVerdict, filterTiles, tileState } from "../js/appsview.js";

/** @param {string} name @param {string} group @param {string} stack */
const tile = (name, group, stack) => ({
  stack,
  host: `${name.toLowerCase()}.example.dev`,
  url: `https://${name.toLowerCase()}.example.dev/`,
  name,
  group,
  order: 0,
  description: `${name} app`,
  lines: [],
});
const TILES = [
  tile("Jellyfin", "Media", "films"),
  tile("Sonarr", "Media", "films"),
  tile("JobTracker", "Work", "kp-soft"),
  tile("Traefik", "Infrastructure", "gateway"),
];

/** @param {any[]} targets */
const watchOf = (targets) => new Map(targets.map((t) => [t.key, t]));

test("redesign-stacks: a tile's dot is its own probe, else its stack's container, else not watched", () => {
  const w = watchOf([
    {
      key: "tile:jellyfin.example.dev",
      state: "down",
      down: true,
      checked_at: 5,
    },
    { key: "stack:films", state: "up", checked_at: 6 },
    { key: "stack:gateway", state: "deploying", checked_at: 6 },
    { key: "tile:jobtracker.example.dev", state: "flaky", failing_since: 3 },
  ]);
  assert.deepEqual(
    [tileState(w, TILES[0]).state, tileState(w, TILES[0]).via],
    ["down", "tile"],
  );
  assert.deepEqual(
    [tileState(w, TILES[1]).state, tileState(w, TILES[1]).via],
    ["up", "stack"],
  );
  assert.equal(tileState(w, TILES[2]).tone, "warn");
  assert.equal(tileState(w, TILES[3]).word, "deploying");
  assert.equal(tileState(new Map(), TILES[0]).state, "unwatched");
});

test("redesign-stacks: the search matches name, address, description and stack", () => {
  assert.deepEqual(
    filterTiles(TILES, "tr").map((t) => t.name),
    ["JobTracker", "Traefik"],
  );
  assert.deepEqual(
    filterTiles(TILES, "films").map((t) => t.name),
    ["Jellyfin", "Sonarr"],
  );
  assert.equal(filterTiles(TILES, "  ").length, 4);
});

test("redesign-stacks: the verdict names what is down first, and counts exactly", () => {
  const up = watchOf([
    { key: "stack:films", state: "up", checked_at: 10 },
    { key: "stack:kp-soft", state: "up", checked_at: 12 },
    { key: "stack:gateway", state: "up", checked_at: 11 },
  ]);
  assert.deepEqual(appsVerdict(TILES, up), {
    tone: "ok",
    text: "All 4 apps answer",
    checkedAt: 12,
  });
  const down = watchOf([
    { key: "tile:sonarr.example.dev", state: "down", down: true },
    { key: "stack:films", state: "up" },
  ]);
  assert.equal(appsVerdict(TILES, down).text, "1 app is down");
  assert.equal(appsVerdict(TILES, down).tone, "bad");
  assert.equal(appsVerdict(TILES, new Map()).text, "4 apps · none watched yet");
  const part = watchOf([{ key: "stack:films", state: "up", checked_at: 1 }]);
  assert.equal(
    appsVerdict(TILES, part).text,
    "2 of 4 apps answer · 2 not watched",
  );
});
