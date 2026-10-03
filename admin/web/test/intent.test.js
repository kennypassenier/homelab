// feat-shell-2 (redesign 3.71.0, FLOWS.md §1.2): the command palette finds
// pages AND actions by intent — words in any order, each matching by
// prefix — grouped Inbox → Do → Go to → Theme, the open stack's own first.
// Before 3.71.0 "update gateway", "gateway update" and "logs gateway" all
// answered "No commands" (one contiguous substring was all it matched).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionCommands,
  allCommands,
  editCommands,
  inboxCommands,
  registerCommands,
  stackCommands,
} from "../js/commands.js";
import {
  MAX_SHOWN,
  matchIntent,
  noMatchText,
  rankCommands,
  score,
  words,
} from "../js/intent.js";

/** @param {string} action @param {Partial<import("../js/actionforms.js").CatalogEntry>} [o] */
const entry = (action, o = {}) =>
  /** @type {import("../js/actionforms.js").CatalogEntry} */ ({
    action,
    target: "stack",
    label: action,
    what: `what ${action} does`,
    scope: "stack",
    needs: "",
    args: [],
    confirm: false,
    refused_for_self: false,
    destructive: false,
    ...o,
  });

const catalog = {
  host_target: "_host",
  self_stack: "admin",
  actions: [
    entry("update", { label: "Update" }),
    entry("backup", { label: "Back up" }),
    entry("restore", { label: "Restore" }),
    entry("restore-native", { label: "Restore (native)" }),
    entry("deploy", { label: "Deploy" }),
    entry("update-host", { target: "host", label: "Update the host" }),
    entry("patch", { target: "host", label: "Patch the fleet" }),
  ],
};
const fleet = /** @type {any} */ ({
  stacks: [
    { name: "admin", vmid: 120 },
    { name: "gateway", vmid: 104 },
    { name: "jellyfin", vmid: 106 },
    { name: "kyu", vmid: 109, native: true },
  ],
});

/** Every provider the shell registers, with the hub of `here` open. */
function commandsFor(/** @type {string | null} */ here) {
  const offs = [
    registerCommands(
      "t-inbox",
      inboxCommands(() => [
        {
          key: "notice:1",
          severity: "bad",
          title: "Backup of gateway failed",
          why: "the nightly round could not reach its repository",
          href: "/inbox#notice-1",
          stack: "gateway",
          at: 1,
          source: "notices",
        },
      ]),
    ),
    registerCommands(
      "t-actions",
      actionCommands(
        () => catalog,
        () => here,
        () => {},
      ),
    ),
    registerCommands(
      "t-edit",
      editCommands(
        () => here,
        () => {},
      ),
    ),
    registerCommands("t-stacks", stackCommands),
  ];
  const all = allCommands({ fleet, themes: [], theme: null });
  offs.forEach((f) => f());
  return all;
}

/** The rows the palette shows for a query, in order, as labels. */
const shown = (
  /** @type {string} */ q,
  here = /** @type {string | null} */ (null),
) =>
  rankCommands(commandsFor(here), q, { here }).flatMap((g) =>
    g.commands.map((c) => `${g.group}: ${c.label}`),
  );

test("words: lower case, hyphenated words also by their parts", () => {
  assert.deepEqual(words("Update · kp-soft"), [
    "update",
    "kp-soft",
    "kp",
    "soft",
  ]);
  assert.deepEqual(words("Restore (native)"), ["restore", "native"]);
});

test("feat-shell-2: every typed word must start a word of the command, in any order", () => {
  assert.equal(matchIntent("Update · jellyfin", "update jellyfin"), true);
  assert.equal(matchIntent("Update · jellyfin", "jellyfin update"), true);
  assert.equal(matchIntent("Update · jellyfin", "upd jel"), true);
  assert.equal(matchIntent("Update · jellyfin", "update gateway"), false);
  assert.equal(matchIntent("gateway · Logs", "logs gateway"), true);
  assert.equal(matchIntent("Update · kp-soft", "update kp"), true);
  assert.equal(matchIntent("anything", ""), true);
});

test("feat-shell-2: Ctrl K finds an action by intent — the cases that answered No commands", () => {
  // "update jellyfin" and its reverse: the action itself, first in Do.
  for (const q of ["update jellyfin", "jellyfin update"]) {
    const rows = shown(q);
    assert.equal(rows[0], "Do: Update · jellyfin", q);
  }
  // "gateway logs" / "logs gateway": the hub's Logs tab.
  for (const q of ["gateway logs", "logs gateway"])
    assert.ok(shown(q).includes("Go to: gateway · Logs"), q);
  // A verb alone lists that verb on every stack it applies to, plus the
  // host's own "Update the host".
  const upd = shown("update");
  assert.ok(upd.includes("Do: Update · gateway"));
  assert.ok(upd.includes("Do: Update the host"));
  // "Change a secret in gateway" by its own words.
  assert.ok(shown("secret gateway").includes("Do: Change a secret in gateway"));
  assert.ok(shown("deploy all").includes("Do: Deploy all changes"));
});

test("feat-shell-2: groups run Inbox → Do → Go to, the open stack's own first", () => {
  const groups = rankCommands(commandsFor("gateway"), "gateway", {
    here: "gateway",
  }).map((g) => g.group);
  assert.deepEqual(groups, ["Inbox", "Do", "Go to"]);
  const rows = shown("update", "jellyfin");
  assert.equal(rows[0], "Do: Update · jellyfin");
});

test("feat-shell-2: an action that does not apply to a stack is left out", () => {
  const rows = shown("restore");
  assert.ok(rows.includes("Do: Restore · gateway"));
  assert.ok(!rows.includes("Do: Restore (native) · gateway"));
  assert.ok(rows.includes("Do: Restore (native) · kyu"));
});

test("feat-shell-2: nothing typed shows a short start, never every stack's actions", () => {
  const rows = shown("");
  assert.ok(rows.includes("Inbox: Backup of gateway failed"));
  assert.ok(rows.includes("Do: New stack…"));
  assert.ok(rows.includes("Do: Deploy all changes"));
  assert.ok(rows.includes("Go to: gateway"));
  assert.ok(!rows.some((r) => r.startsWith("Do: Update ·")));
  assert.ok(!rows.includes("Go to: gateway · Logs"));
  // On a hub, that stack's own actions are in the start too.
  assert.ok(shown("", "gateway").includes("Do: Update · gateway"));
  assert.ok(shown("").length <= MAX_SHOWN);
});

test("feat-shell-2: a label word outranks a hidden one", () => {
  const want = ["update"];
  const label = score(
    { id: "a", group: "Do", label: "Update · x", words: "" },
    want,
  );
  const hidden = score(
    { id: "b", group: "Do", label: "Pull · x", words: "update" },
    want,
  );
  assert.ok(label > hidden && hidden > 0);
  assert.equal(score({ id: "c", group: "Do", label: "Deploy" }, want), -1);
});

test("feat-shell-2: when nothing matches it says why, naming what the verb works on", () => {
  assert.deepEqual(shown("update nosuchstack"), []);
  assert.equal(
    noMatchText("update nosuchstack", {
      verbs: ["Update", "Back up"],
      stacks: ["admin", "gateway"],
    }),
    'There is no stack called "nosuchstack". Update works on: admin, gateway.',
  );
  assert.match(
    noMatchText("zzz", { verbs: ["Update"], stacks: ["admin"] }),
    /Nothing matches/,
  );
});
