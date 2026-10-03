// feat-overview-3, nav-decisions: the palette's entries are a registry;
// `pageCommands` renders from the page registry (pages.js), the same one
// the nav bar does, so this primes it with `setPages` instead of a
// hand-written page list.
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
import { setPages } from "../js/pages.js";

setPages({
  app: "admin",
  brand: { title: "Homelab", href: "/overview" },
  home: "/",
  pages: [
    {
      id: "home",
      title: "Apps",
      path: "/",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "host",
      title: "Host",
      path: "/host",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
    {
      id: "activity",
      title: "Activity",
      path: "/activity",
      group: null,
      order: 0,
      nav: true,
      source: "app",
      render: "app",
    },
  ],
});

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
  // feat-shell-2: the areas the registry lists, every page inside an
  // area (areas.js), every stack's hub and tabs, all in "Go to".
  assert.ok(ids.includes("page:home"));
  assert.ok(ids.includes("page:activity"));
  assert.ok(ids.includes("page:host"), "System's Host page");
  assert.ok(ids.includes("page:planned"), "Activity's Planned view");
  assert.ok(!ids.includes("page:inbox"), "an area the registry did not list");
  assert.ok(!ids.includes("page:timeline"), "Timeline merged into Activity");
  assert.ok(ids.includes("stack:media:overview"));
  assert.ok(ids.includes("stack:media:logs"));
  assert.equal(all.find((c) => c.id === "page:activity")?.keys, "g a");
  assert.equal(
    all.find((c) => c.id === "page:planned")?.href,
    "/activity?view=planned",
  );
  assert.equal(
    all.find((c) => c.id === "stack:media:logs")?.href,
    "/stacks/media/logs",
  );
  assert.equal(
    all.find((c) => c.id === "stack:media:overview")?.hint,
    "stack · CT 106",
  );
  const dark = all.find((c) => c.id === "theme:dark");
  assert.equal(dark?.hint, "current");
  dark?.run?.();
  assert.deepEqual(chosen, ["dark"]);
  assert.deepEqual(
    grouped(all).map((g) => g.group),
    ["Go to", "Theme"],
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
