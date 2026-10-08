// redesign-stacks (3.71.0, the approved Apps demo): the Apps page's view
// model — a tile's dot from the minute watch, the launch search, the board
// with Starred on top, the highlight and the verdict line.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  appsVerdict,
  board,
  filterTiles,
  hueOf,
  initials,
  markParts,
  packColumns,
  packGroups,
  readStars,
  tileState,
} from "../js/appsview.js";

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

test("redesign-stacks: Starred sits on top only while nothing is searched, and a starred tile leaves its group", () => {
  const stars = new Set(["traefik.example.dev"]);
  const b = board(TILES, stars, "");
  assert.deepEqual(
    b.map((g) => [g.group, g.tiles.length, g.wide]),
    [
      ["Starred", 1, true],
      ["Media", 2, false],
      ["Work", 1, false],
    ],
  );
  const s = board(TILES, stars, "tr");
  assert.deepEqual(
    s.map((g) => [g.group, g.tiles.map((t) => t.name), g.wide]),
    [
      ["Work", ["JobTracker"], true],
      ["Infrastructure", ["Traefik"], true],
    ],
  );
});

test("redesign-stacks: the highlight cuts around the first match, any case", () => {
  assert.deepEqual(markParts("JobTracker", "TR"), [
    { text: "Job", hit: false },
    { text: "Tr", hit: true },
    { text: "acker", hit: false },
  ]);
  assert.deepEqual(markParts("Sonarr", "x"), [{ text: "Sonarr", hit: false }]);
  assert.deepEqual(markParts("Sonarr", ""), [{ text: "Sonarr", hit: false }]);
});

test("redesign-stacks: a tile's colour and letters are stable", () => {
  assert.equal(hueOf("Jellyfin"), hueOf("Jellyfin"));
  assert.ok(hueOf("Jellyfin") >= 1 && hueOf("Jellyfin") <= 5);
  assert.equal(initials("Jellyfin"), "Je");
  assert.equal(initials(" "), "?");
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

test("redesign-stacks: stars kept in this browser read back, anything else reads as none", () => {
  assert.deepEqual([...readStars('["a","b",3]')], ["a", "b"]);
  assert.deepEqual([...readStars(null)], []);
  assert.deepEqual([...readStars("{")], []);
  assert.deepEqual([...readStars('{"a":1}')], []);
});

test("fix-371-1: the board packs each group into the column shortest by tile count", () => {
  // The fleet's own groups, as the approved demo packed them.
  const g = [7, 3, 3, 5, 5].map((tiles) => ({ tiles }));
  assert.deepEqual(packGroups(g, 3), [[0], [1, 3], [2, 4]]);
  assert.deepEqual(packGroups(g, 2), [
    [0, 3],
    [1, 2, 4],
  ]);
  assert.deepEqual(packGroups(g, 1), [[0, 1, 2, 3, 4]]);
  // A folded group counts as its title alone: the next one fills under it.
  const folded = g.map((x, i) => ({ ...x, folded: i === 0 }));
  assert.deepEqual(packGroups(folded, 3), [[0, 3], [1, 4], [2]]);
  // Column count: 22 rem columns from a 53 rem board, 18 rem below.
  assert.equal(packColumns(1400, 24, 16), 3);
  assert.equal(packColumns(1000, 24, 16), 2);
  assert.equal(packColumns(700, 24, 16), 2);
  assert.equal(packColumns(358, 16, 16), 1);
});
