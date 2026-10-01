// Milestone follow (feat-platform-10): the driven replay's pure half, and
// the one form description both sides read.
import { test } from "node:test";
import assert from "node:assert/strict";
import { actionForm, checkValues } from "../js/actionforms.js";
import {
  checkFields,
  latchFileFields,
  latchFileProblems,
  parseKey,
  ruleFields,
  ruleProblems,
  settingsExtForm,
  settingsForm,
  tileProblems,
} from "../js/editforms.js";
import {
  animate,
  badgeText,
  catchUp,
  isActive,
  plan,
  readFollow,
} from "../js/driveview.js";
import { NAV, OTHER_PAGES, STACK_TABS } from "../js/router.js";
import SPEC from "../js/formspec.json" with { type: "json" };
import CASES from "./formspec-cases.json" with { type: "json" };

/** @typedef {import("../js/driveview.js").DriveState} DriveState */

/** @param {Partial<DriveState>} [o] @returns {DriveState} */
const state = (o = {}) => ({
  active: true,
  by: "wsl",
  seq: 3,
  page: "/app/stacks/media",
  form: {
    id: "action:deploy",
    action: "deploy",
    stack: "media",
    title: "Deploy · media",
    steps: ["review"],
    step: "review",
    step_index: 0,
    values: { force: false },
    errors: {},
    run_error: null,
    job: null,
    fields: [
      {
        id: "act-force",
        name: "force",
        kind: "check",
        value: false,
        error: null,
        shown: false,
        step: "review",
      },
    ],
    buttons: ["confirm", "close"],
  },
  last_at: 1000,
  idle_s: 600,
  ...o,
});

/** @param {number} seq @param {any} step @param {DriveState} s */
const ev = (seq, step, s, applied = true) => ({
  seq,
  step,
  applied,
  refusal: applied ? null : { what: "ui type", why: "no field", fix: "x" },
  state: s,
});

test("a tab that is off never changes: no operation for any event", () => {
  const local = { seq: 2, page: "/app/jobs", form: null };
  for (const step of [
    { do: "goto", path: "/app/stacks/media" },
    { do: "open", form: "deploy", target: "media" },
    { do: "type", field: "act-confirm", text: "media" },
    { do: "press", button: "confirm" },
  ]) {
    const p = plan(local, ev(3, step, state()), false);
    assert.deepEqual(p.ops, []);
    assert.equal(p.local, local, "the tab's own place is kept");
  }
  // Refused steps say nothing to a tab that does not follow either.
  assert.deepEqual(
    plan(local, ev(3, { do: "x" }, state(), false), false).ops,
    [],
  );
  assert.equal(readFollow(null), false, "off by default");
  assert.equal(readFollow({ getItem: () => "on" }), true);
});

test("a tab that follows animates the next step and catches up after a gap", () => {
  const before = { seq: 2, page: "/app/jobs", form: null };
  const opened = plan(
    before,
    ev(3, { do: "open", form: "deploy", target: "media" }, state()),
    true,
  );
  assert.deepEqual(opened.ops, [
    { op: "goto", path: "/app/stacks/media" },
    { op: "open", action: "deploy", stack: "media" },
    { op: "sync" },
  ]);
  assert.deepEqual(opened.local, {
    seq: 3,
    page: "/app/stacks/media",
    form: { action: "deploy", stack: "media" },
  });
  const typed = plan(
    opened.local,
    ev(4, { do: "check", field: "act-force", on: true }, state({ seq: 4 })),
    true,
  );
  assert.deepEqual(typed.ops, [
    { op: "set", name: "force", id: "act-force", value: true },
  ]);
  const pressed = plan(
    typed.local,
    ev(5, { do: "press", button: "confirm" }, state({ seq: 5 })),
    true,
  );
  assert.deepEqual(pressed.ops, [
    { op: "press", button: "confirm" },
    { op: "sync" },
  ]);
  // A tab that turns on mid-drive (or missed an event) catches up at once.
  const late = plan(
    { seq: -1, page: "/app/", form: null },
    ev(5, { do: "press", button: "confirm" }, state({ seq: 5 })),
    true,
  );
  assert.deepEqual(late.ops, [
    { op: "goto", path: "/app/stacks/media" },
    { op: "open", action: "deploy", stack: "media" },
    { op: "set", name: "force", id: "act-force", value: false },
    { op: "sync" },
  ]);
  // Another form open in the tab is closed first.
  assert.equal(
    catchUp(
      {
        seq: 0,
        page: "/app/stacks/media",
        form: { action: "backup", stack: "media" },
      },
      state(),
    )[0].op,
    "close",
  );
  const refused = plan(
    typed.local,
    ev(4, { do: "type" }, state({ seq: 4 }), false),
    true,
  );
  assert.equal(refused.ops[0].op, "note");
  assert.equal(refused.local, typed.local);
});

test("the badge names the stack and the action while Claude drives, and ends", () => {
  assert.equal(badgeText(state(), 1010), "Claude is working on media: Deploy");
  assert.equal(badgeText(state(), 1700), null, "a silent driver lets go");
  assert.equal(badgeText(state({ active: false }), 1010), null);
  assert.equal(isActive(null, 0), false);
  assert.match(String(badgeText(state({ form: null }), 1010)), /dashboard/);
});

test("after a dashboard restart the re-read state ends a stale badge", () => {
  // Before: Claude drove media. The restarted dashboard answers with a
  // fresh, inactive state (seq from 0, no form): nothing is driving.
  assert.ok(badgeText(state(), 1010));
  const fresh = state({ active: false, by: null, seq: 0, form: null });
  assert.equal(isActive(fresh, 1010), false);
  assert.equal(badgeText(fresh, 1010), null);
});

test("the form description is one file: the browser's checks match the cases the server runs", () => {
  for (const c of CASES.cases) {
    const entry = /** @type {import("../js/actionforms.js").CatalogEntry} */ ({
      action: c.action,
      target: "stack",
      label: c.action,
      what: "",
      scope: "operate",
      needs: "nothing",
      args: /** @type {any} */ (c.args),
      confirm: c.confirm,
      refused_for_self: false,
    });
    const form = actionForm(entry, { stack: c.stack, selfStack: "admin" });
    const got = checkValues(
      form,
      /** @type {any} */ (c.values),
      /** @type {any} */ ("step" in c ? c.step : undefined),
    );
    assert.deepEqual(got, c.errors, JSON.stringify(c));
  }
  // The pages a driven `goto` may name are the router's.
  const pages = [...NAV, ...OTHER_PAGES]
    .map((n) => n.href.replace(/^\/app\/?/, ""))
    .sort();
  assert.deepEqual([...SPEC.pages].sort(), pages);
  assert.deepEqual(
    SPEC.stack_tabs,
    STACK_TABS.map((t) => t.tab),
  );
});

/** An edit form's state, as the dashboard's server sends it. */
const editState = (/** @type {Partial<DriveState>} */ o = {}) =>
  state({
    page: "/app/stacks/admin/firewall",
    form: {
      .../** @type {import("../js/driveview.js").DriveForm} */ (state().form),
      id: "edit:firewall:admin",
      action: "firewall",
      stack: "admin",
      title: "Firewall · admin",
      steps: ["rules", "plan", "commit"],
      step: "rules",
      fields: [
        {
          id: "rule-peer",
          name: "peer",
          kind: "text",
          value: "10.10.10.4",
          error: null,
          shown: true,
          step: "rule",
        },
      ],
      edit: { family: "firewall", guarded: 0 },
    },
    ...o,
  });

test("an edit form catches up by opening its own editor and syncing it", () => {
  const ops = catchUp({ seq: -1, page: "/app/", form: null }, editState());
  assert.deepEqual(ops, [
    { op: "goto", path: "/app/stacks/admin/firewall" },
    { op: "open", action: "firewall", stack: "admin", edit: true },
    { op: "sync" },
  ]);
});

test("row, edit and a typed field name the field by its id", () => {
  const local = {
    seq: 3,
    page: "/app/stacks/admin/firewall",
    form: { action: "firewall", stack: "admin" },
  };
  const s = editState();
  assert.deepEqual(animate(local, { do: "row", op: "add" }, s), [
    { op: "row", row: "add", target: undefined },
    { op: "sync" },
  ]);
  assert.deepEqual(
    animate(local, { do: "type", field: "rule-peer", text: "10.10.10.4" }, s),
    [{ op: "type", name: "peer", id: "rule-peer", text: "10.10.10.4" }],
  );
  assert.deepEqual(
    animate(local, { do: "edit", field: "rule-peer", text: "a\nb" }, s),
    [{ op: "set", name: "peer", id: "rule-peer", value: "a\nb" }],
  );
});

test("a press that hands over to another form closes this one and opens that", () => {
  const local = {
    seq: 3,
    page: "/app/stacks/media",
    form: { action: "rollback", stack: "media" },
  };
  const ops = animate(local, { do: "press", button: "next" }, state());
  assert.deepEqual(
    ops.map((o) => o.op),
    ["press", "close", "open", "set", "sync"],
  );
  assert.equal(
    badgeText(editState({ form: null }), 1010)?.includes("dashboard"),
    true,
  );
});

test("the edit checks are the server's: the same cases give the same words", () => {
  for (const c of CASES.edit_cases) {
    /** @type {any} */
    const v = c.values;
    let got;
    if (c.check === "settings") {
      const form = settingsForm("kp-soft", /** @type {any} */ (c.manifest), {});
      got = checkFields(form, v);
    } else if (c.check === "rule") {
      const steps = [{ id: "rule", label: "Rule", fields: ruleFields(null) }];
      got = { ...checkFields({ steps }, v), ...ruleProblems(v) };
    } else if (c.check === "tile") {
      got = tileProblems(v);
    } else if (c.check === "settings_ext") {
      const form = settingsExtForm("kp-soft", /** @type {any} */ (c.manifest));
      got = checkFields(form, v);
    } else if (c.check === "latch_file") {
      const m = /** @type {any} */ ({ natives: c.natives ?? [] });
      const steps = [
        { id: "row", label: "Row", fields: latchFileFields(m, null) },
      ];
      got = { ...checkFields({ steps }, v), ...latchFileProblems(v) };
    } else {
      const p = parseKey(
        /** @type {any} */ ({ kind: c.kind }),
        /** @type {any} */ (v.value),
      );
      got = p.ok ? { value: p.value } : { why: p.why };
    }
    assert.deepEqual(got, c.errors, JSON.stringify(c));
  }
});

test("feat-native-1 / feat-preset-1: the driven forms' field ids are the browser's own", () => {
  // The native/add-native/preset forms are hand-built DOM (editpanels.js,
  // presetseditor.js), not editforms.js's Field/checkFields machinery like
  // Settings — so this is the id contract itself, not a computed form: if
  // formspec.json's ids drift from the literal ids those two files use,
  // `homelab ui open native|add-native|preset|new-preset` breaks silently
  // (byId() in editdrive.js finds nothing).
  const ids = (/** @type {{id: string}[]} */ list) => list.map((f) => f.id);
  assert.deepEqual(ids(SPEC.edit.native), [
    "native-unit",
    "native-binary",
    "native-env-file",
    "native-data-dirs",
    "native-update-cmd",
    "native-stateless",
    "native-restore-note",
    "native-release-repo",
    "native-release-asset",
    "native-backup-newest",
    "native-backup-pause",
    "native-update-policy",
    "native-metrics",
  ]);
  assert.deepEqual(ids(SPEC.edit.add_native), [
    "add-native-unit",
    "add-native-binary",
    "add-native-env-file",
    "add-native-data-dirs",
    "add-native-notify",
  ]);
  assert.deepEqual(ids(SPEC.edit.preset_meta), [
    "preset-description",
    "preset-ram-mb",
    "preset-cores",
    "preset-disk-gb",
    "preset-features",
    "preset-gpu",
    "preset-vpn",
  ]);
  assert.equal(SPEC.edit.new_preset_name.id, "new-preset-name");
});
