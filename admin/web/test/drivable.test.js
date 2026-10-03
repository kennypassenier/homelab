// fix-239: Live view reaches every page control (drivable.js), and a click
// step is taken by the tab itself (driveview.js `animate`).
import { test } from "node:test";
import assert from "node:assert/strict";
import { control, controls, declare, pick } from "../js/drivable.js";
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
