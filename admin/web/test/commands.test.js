// feat-overview-3: the palette's entries are a registry.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  allCommands,
  grouped,
  pageCommands,
  registerCommands,
  stackCommands,
  themeCommands,
} from "../js/commands.js";

/** @type {import("../js/fleet.js").Fleet} */
const fleet = /** @type {any} */ ({
  measured_at: 1,
  host: {},
  counts: { stacks: 2, online: 2, parked: 0 },
  stacks: [
    { name: "gateway", vmid: 104 },
    { name: "media", vmid: 106 },
  ],
});
const themes = [
  { name: "formal", label: "Formal", dark: false },
  { name: "dark", label: "Dark", dark: true },
];

test("pages, every stack with each tab, and the themes", () => {
  const chosen = /** @type {string[]} */ ([]);
  const off = [
    registerCommands("pages", pageCommands),
    registerCommands("stacks", stackCommands),
    registerCommands(
      "themes",
      themeCommands((n) => chosen.push(n)),
    ),
  ];
  const all = allCommands({ fleet, themes, theme: "dark" });
  const ids = all.map((c) => c.id);
  assert.ok(ids.includes("page:host"));
  assert.ok(ids.includes("page:timeline"));
  assert.ok(ids.includes("stack:media:overview"));
  assert.ok(ids.includes("stack:media:logs"));
  assert.equal(all.find((c) => c.id === "page:host")?.keys, "g h");
  assert.equal(
    all.find((c) => c.id === "stack:media:logs")?.href,
    "/app/stacks/media/logs",
  );
  assert.equal(
    all.find((c) => c.id === "stack:media:overview")?.hint,
    "CT 106",
  );
  const dark = all.find((c) => c.id === "theme:dark");
  assert.equal(dark?.hint, "current");
  dark?.run?.();
  assert.deepEqual(chosen, ["dark"]);
  assert.deepEqual(
    grouped(all).map((g) => g.group),
    ["Pages", "Stacks", "Theme"],
  );
  off.forEach((f) => f());
  assert.deepEqual(allCommands({ fleet, themes, theme: null }), []);
});

test("a later milestone's provider joins, a broken one is skipped, ids are unique", () => {
  const offA = registerCommands("act", () => [
    {
      id: "deploy:media",
      group: "Actions",
      label: "Deploy media",
      run: () => {},
    },
    { id: "deploy:media", group: "Actions", label: "twice", run: () => {} },
  ]);
  const offB = registerCommands("broken", () => {
    throw new Error("no");
  });
  const all = allCommands({ fleet: null, themes: [], theme: null });
  assert.deepEqual(
    all.map((c) => c.label),
    ["Deploy media"],
  );
  offA();
  offB();
  // No fleet yet: no stacks, and no error.
  const offS = registerCommands("stacks", stackCommands);
  assert.deepEqual(allCommands({ fleet: null, themes: [], theme: null }), []);
  offS();
});
