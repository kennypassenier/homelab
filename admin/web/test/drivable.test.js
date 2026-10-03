// fix-239: Live view reaches every page control (drivable.js), and a click
// step is taken by the tab itself (driveview.js `animate`).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  clickLine,
  closest,
  control,
  controls,
  declare,
  declareField,
  currentField,
  fields,
  howToReach,
  pick,
  reachLine,
  resolve,
} from "../js/drivable.js";
import { animate, plan, targetOf } from "../js/driveview.js";

const here = [
  { id: "pin-update", row: "media/web/web", label: "Update to 2.0" },
  { id: "pin-update", row: "media/api/api", label: "Update to v3" },
  { id: "new-schedule", row: null, label: "New schedule" },
];

test("fix-239: a click names its control, and its row when it repeats", () => {
  assert.deepEqual(pick({ id: "new-schedule", row: null }, here), {
    index: 2,
  });
  assert.deepEqual(pick({ id: "pin-update", row: "media/api/api" }, here), {
    index: 1,
  });
  const many = pick({ id: "pin-update", row: null }, here);
  assert.ok("why" in many && many.fix.includes("media/web/web"));
  const wrongRow = pick({ id: "pin-update", row: "media/x/x" }, here);
  assert.ok("why" in wrongRow && wrongRow.fix.includes("media/api/api"));
});

test("fix-239: an unknown control is refused with the controls on screen", () => {
  const r = pick({ id: "snooze", row: null }, here);
  assert.ok("why" in r);
  assert.match(r.fix, /new-schedule, pin-update/);
});

test("fix-239: a dialog's own button is found by its label too", () => {
  const dialog = [
    { id: "", row: null, label: "Cancel" },
    { id: "", row: null, label: "Create" },
  ];
  assert.deepEqual(pick({ id: "create", row: null }, dialog), { index: 1 });
});

test("fix-239: a control is declared once, with a page and what it does", () => {
  const id = declare({
    id: "test-only-control",
    page: "jobs",
    opens: "run",
    what: "a test",
  });
  assert.ok(controls().some((c) => c.id === id));
  assert.throws(() =>
    declare({ id, page: "jobs", opens: "run", what: "again" }),
  );
  assert.throws(() =>
    declare({ id: "Bad Id", page: "jobs", opens: "run", what: "x" }),
  );
});

test("fix-239: a click, and a field with no server form open, is the tab's step", () => {
  const local = { seq: 3, page: "/fleetview", form: null };
  /** @type {any} */
  const s = { seq: 4, page: "/fleetview", form: null, page_dialog: null };
  const click = { do: "click", control: "pin-update", row: "media/web/web" };
  assert.deepEqual(animate(local, click, s), [{ op: "tab", step: click }]);
  const tick = { do: "check", field: "pin-major-read", on: true };
  assert.deepEqual(animate(local, tick, s), [{ op: "tab", step: tick }]);
  const press = { do: "press", button: "confirm" };
  const inDialog = { ...s, page_dialog: { control: "pin-update", title: "x" } };
  assert.deepEqual(animate(local, press, inDialog), [
    { op: "tab", step: press },
  ]);
  assert.deepEqual(targetOf(click), {
    kind: "control",
    control: "pin-update",
    row: "media/web/web",
  });
});

test("fix-239: a tab that has not caught up still takes a click after catching up", () => {
  const local = { seq: -1, page: "/jobs", form: null };
  /** @type {any} */
  const state = { seq: 9, page: "/jobs", form: null, page_dialog: null };
  const click = { do: "click", control: "new-schedule" };
  const ev = { seq: 9, step: click, applied: true, refusal: null, state };
  const { ops } = plan(local, ev, true);
  assert.deepEqual(ops.at(-1), { op: "tab", step: click });
  assert.deepEqual(
    plan(local, ev, false).ops,
    [],
    "a tab that does not follow",
  );
});

// drive-reach (Kenny, 2026-10-03: "Claude must always be able to reach
// every control, also after a control is renamed or moved").

test("drive-reach: an old control name resolves to what it became, with its menu press", () => {
  declare({
    id: "test-reach-menu",
    page: "jobs",
    opens: "dialog",
    row: "<job>",
    what: "open one test job's menu",
    was: [{ id: "test-reach-edit", press: "edit" }, "test-reach-old"],
  });
  assert.deepEqual(
    (({ control, was, press }) => ({ id: control.id, was, press }))(
      /** @type {any} */ (resolve("test-reach-edit")),
    ),
    { id: "test-reach-menu", was: "test-reach-edit", press: "edit" },
  );
  assert.equal(resolve("test-reach-old")?.press, null);
  assert.equal(resolve("test-reach-menu")?.was, null);
  assert.equal(resolve("test-reach-never"), null);
  // An old name is taken once, and never by a new control.
  assert.throws(() =>
    declare({ id: "test-reach-old", page: "jobs", opens: "run", what: "x" }),
  );
  assert.throws(() =>
    declare({
      id: "test-reach-other",
      page: "jobs",
      opens: "run",
      what: "x",
      was: ["test-reach-edit"],
    }),
  );
});

test("drive-reach: a mistyped or guessed name finds the closest declared controls", () => {
  declare({
    id: "test-reach-snooze",
    page: "jobs",
    opens: "run",
    what: "pause the test notices for an hour",
  });
  assert.equal(closest("test-reach-snoze")[0]?.id, "test-reach-snooze");
  assert.ok(
    closest("pause notices").some((c) => c.id === "test-reach-snooze"),
    "by what it does",
  );
  assert.ok(
    closest("test-reach-edit").some((c) => c.id === "test-reach-menu"),
    "by an old name",
  );
  assert.deepEqual(closest("zzzz-qqqq-xxxx"), []);
});

test("drive-reach: a refusal can say how to reach a control, step by step", () => {
  declare({
    id: "test-reach-undo",
    page: "jobs",
    opens: "run",
    what: "undo the test switch",
    shows: "while its Undo toast shows",
    reach: [{ do: "click", control: "test-reach-menu", row: "*" }],
  });
  const c = /** @type {any} */ (resolve("test-reach-undo")).control;
  assert.equal(
    howToReach(c),
    "homelab ui click test-reach-menu <row>; then homelab ui click test-reach-undo",
  );
  assert.equal(
    clickLine(/** @type {any} */ (resolve("test-reach-menu")).control),
    "homelab ui click test-reach-menu <job>",
  );
  assert.equal(
    reachLine({ do: "type", field: "secret-value", text: "x" }),
    "homelab ui type secret-value x",
  );
});

test("drive-reach: a goto to an old address goes through the browser's own redirect, and its view is no second goto", () => {
  const local = { seq: 3, page: "/apps", form: null };
  /** @type {any} */
  const s = {
    seq: 4,
    page: "/activity?view=running",
    form: null,
    page_dialog: null,
  };
  const step = { do: "goto", path: "/jobs" };
  // The address the driver named: the router sends it on, query and all.
  assert.deepEqual(animate(local, step, s), [{ op: "goto", path: "/jobs" }]);
  // A tab already on the Activity page needs no goto to catch up.
  const there = { seq: -1, page: "/activity", form: null };
  const ev = { seq: 4, step, applied: true, refusal: null, state: s };
  assert.ok(!plan(there, ev, true).ops.some((o) => o.op === "goto"));
});

test("redesign-activity: an id a redesign renamed (`was`) still reaches the control", () => {
  declare({
    id: "test-merged-menu",
    was: ["test-old-edit", "test-old-delete"],
    page: "test",
    opens: "dialog",
    row: "<id>",
    what: "a menu that replaced two buttons",
  });
  assert.equal(control("test-old-edit")?.id, "test-merged-menu");
  const on = [{ id: "test-merged-menu", row: "a", label: "More" }];
  assert.deepEqual(pick({ id: "test-old-delete", row: "a" }, on), { index: 0 });
  assert.throws(() =>
    declare({
      id: "test-old-edit",
      page: "test",
      opens: "view",
      what: "taken by an alias",
    }),
  );
});

test("drive-reach: a control with no rows drawn twice is one control, pressed once", () => {
  // review M6: only when its declaration says so.
  declare({
    id: "test-close-drawer",
    page: "test",
    opens: "run",
    what: "close the test drawer (its x and its Cancel)",
    twins: true,
  });
  const twice = [
    { id: "test-close-drawer", row: null, label: "×" },
    { id: "test-close-drawer", row: null, label: "Cancel" },
  ];
  assert.deepEqual(pick({ id: "test-close-drawer", row: null }, twice), {
    index: 0,
  });
  const rows = [
    { id: "test-per-row", row: "a", label: "Go" },
    { id: "test-per-row", row: "b", label: "Go" },
  ];
  assert.match(
    String(
      /** @type {any} */ (pick({ id: "test-per-row", row: null }, rows)).why,
    ),
    /on 2 rows/,
  );
});

test("drive-reach: a page field a redesign renamed answers to its old id", () => {
  declareField({
    id: "test-shell-target",
    was: ["test-shell-vmid"],
    page: "test",
    what: "the container a command runs in",
  });
  assert.equal(currentField("test-shell-vmid"), "test-shell-target");
  assert.equal(currentField("test-other-field"), "test-other-field");
  assert.ok(fields().some((f) => f.id === "test-shell-target"));
  assert.throws(() =>
    declareField({ id: "test-shell-vmid", page: "test", what: "taken" }),
  );
});
