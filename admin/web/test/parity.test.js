// The TUI parity round's own view-model logic (no kp components): the new
// action forms as data, today's verdict, the stack flags, the live log's
// filter, the version warnings, the apply plan in words, a shell line.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  actionForm,
  buildArgs,
  checkValues,
  fieldChoices,
  initialValues,
  schedulableActions,
} from "../js/actionforms.js";
import {
  applySummary,
  checkChoices,
  findingRows,
  hostSettingRows,
  lineMatches,
  shellResult,
  stackFlags,
  todayView,
  transferView,
  versionNotes,
} from "../js/parity.js";

/** @param {string} action @param {Partial<import("../js/actionforms.js").CatalogEntry>} o */
const entry = (action, o) =>
  /** @type {import("../js/actionforms.js").CatalogEntry} */ ({
    action,
    target: "host",
    label: action,
    what: `does ${action}`,
    scope: "operate",
    needs: "nothing",
    args: [],
    confirm: false,
    refused_for_self: false,
    ...o,
  });
const ctx = { stack: "_host", selfStack: "admin", hostTarget: "_host" };

test("exec is one step: container and command on the review, no typed name", () => {
  const f = actionForm(
    entry("exec", { args: ["vmid", "command"], scope: "all" }),
    ctx,
  );
  assert.deepEqual(
    f.steps.map((s) => [s.id, s.fields.map((x) => x.id)]),
    [["review", ["act-vmid", "act-command"]]],
  );
  assert.equal(f.confirmName, null);
  assert.ok(f.destructive);
  const v = initialValues(f);
  assert.deepEqual(Object.keys(checkValues(f, v)).sort(), ["command", "vmid"]);
  v.vmid = "10";
  v.command = "df -h";
  assert.match(checkValues(f, v).vmid ?? "", /not a valid/);
  v.vmid = "105";
  assert.deepEqual(checkValues(f, v), {});
  assert.deepEqual(buildArgs(f, v), { vmid: "105", command: "df -h" });
});

test("template-build asks its options first and names the temporary container", () => {
  const f = actionForm(
    entry("template-build", {
      args: ["vmid", "version", "privileged", "base"],
    }),
    ctx,
  );
  assert.deepEqual(
    f.steps.map((s) => s.id),
    ["options", "review"],
  );
  const vmid = f.steps[0].fields.find((x) => x.name === "vmid");
  assert.equal(vmid?.label, "Temporary container number");
  const base = f.steps[0].fields.find((x) => x.name === "base");
  assert.deepEqual(
    base &&
      fieldChoices(base, { templates: ["local:vztmpl/debian-13.tar.zst"] }),
    [
      { value: "", label: "The host's default" },
      {
        value: "local:vztmpl/debian-13.tar.zst",
        label: "local:vztmpl/debian-13.tar.zst",
      },
    ],
  );
});

test("the answer's verdict is its own list; the check comes from the checks", () => {
  const f = actionForm(
    entry("answer-check", { args: ["check", "verdict", "days", "note"] }),
    ctx,
  );
  const fields = f.steps[0].fields;
  const verdict = fields.find((x) => x.name === "verdict");
  assert.deepEqual(verdict && fieldChoices(verdict, {}).map((c) => c.value), [
    "",
    "ok",
    "nok",
    "accept",
  ]);
  const check = fields.find((x) => x.name === "check");
  assert.deepEqual(
    check &&
      fieldChoices(check, { checks: [{ id: "c4bc", label: "media/x: ok?" }] }),
    [
      { value: "", label: "Choose…" },
      { value: "c4bc", label: "media/x: ok?" },
    ],
  );
  // A schedule cannot know an answer in advance; the host update it can.
  const cat = {
    host_target: "_host",
    self_stack: "admin",
    actions: [
      entry("answer-check", { args: ["check", "verdict", "days", "note"] }),
      entry("exec", { args: ["vmid", "command"] }),
      entry("update-host", { args: ["tag"] }),
    ],
  };
  assert.deepEqual(
    schedulableActions(cat).map((a) => a.action),
    ["update-host"],
  );
});

test("today puts broken first and says why the verdict is what it is", () => {
  const v = todayView({
    today: {
      items: [
        { level: "Attention", source: "check", what: "a", remedy: "x" },
        { level: "Broken", source: "doctor", what: "b", remedy: "y" },
      ],
      unread: ["incidents"],
    },
    verdict: "2 things need you",
    needs_you: true,
    stack_files: 14,
  });
  assert.equal(v.tone, "destructive");
  assert.deepEqual(
    v.items.map((i) => i.badge.label),
    ["broken", "attention"],
  );
  assert.deepEqual(v.unread, ["incidents"]);
  const calm = todayView({
    today: { items: [], unread: [] },
    verdict: "Nothing needs you",
    needs_you: false,
    stack_files: 14,
  });
  assert.equal(calm.tone, "success");
  assert.deepEqual(
    findingRows([
      { severity: "Noted", subject: "b", what: "", remedy: "" },
      { severity: "Broken", subject: "a", what: "", remedy: "" },
    ]).map((f) => f.badge.label),
    ["broken", "noted"],
  );
});

test("a stack's flags are the TUI's: [OFF] [CHANGED] [NOENV], each only when true", () => {
  const labels = (/** @type {any} */ s, /** @type {any} */ d) =>
    stackFlags(s, d).map((f) => f.label);
  assert.deepEqual(labels({ enabled: true, env_sealed: true }, "same"), []);
  assert.deepEqual(labels({ enabled: false, env_sealed: false }, "changed"), [
    "OFF",
    "CHANGED",
    "NOENV",
  ]);
  // Not compared is not changed (fix-107).
  assert.deepEqual(
    labels({ enabled: true, env_sealed: true }, "not_compared"),
    [],
  );
});

test("host.toml rows never show a secret's value", () => {
  const rows = hostSettingRows({
    fields: [
      {
        key: "backup_hour",
        group: "Nightly",
        label: "Backup hour",
        default: "off",
        set: true,
        value: 4,
        access: "browser",
      },
      {
        key: "token",
        group: "Access",
        label: "Token",
        default: "",
        set: true,
        value: "should-never-arrive",
        access: "secret",
      },
      {
        key: "listen",
        group: "Access",
        label: "Listen",
        default: "0.0.0.0:8443",
        set: false,
        value: null,
        access: "locked",
      },
    ],
  });
  assert.deepEqual(
    rows.map((r) => [r.key, r.value, r.source]),
    [
      ["backup_hour", "4", "host.toml"],
      ["token", "set (not shown)", "host.toml"],
      ["listen", "default: 0.0.0.0:8443", "default"],
    ],
  );
});

test("the live log filters on source, level and text", () => {
  const l = (/** @type {string} */ source, /** @type {string} */ level) => ({
    seq: 1,
    ts: 0,
    level,
    source,
    msg: "Deploy media: pull",
    req: null,
    by: null,
  });
  const all = { source: "", level: "", q: "" };
  assert.ok(lineMatches(l("media", "info"), all));
  assert.ok(!lineMatches(l("home", "info"), { ...all, source: "media" }));
  assert.ok(!lineMatches(l("media", "info"), { ...all, level: "warn" }));
  assert.ok(lineMatches(l("media", "error"), { ...all, level: "warn" }));
  assert.ok(lineMatches(l("media", "info"), { ...all, q: "PULL" }));
  assert.ok(!lineMatches(l("media", "info"), { ...all, q: "backup" }));
  assert.deepEqual(
    transferView({
      op: "backup-media",
      label: "restic",
      done: 512 * 1024 * 1024,
      total: 2048 * 1024 * 1024,
    }),
    { label: "backup-media · restic", pct: 25, text: "512 MB of 2.0 GB" },
  );
});

test("the version warnings, the apply plan and a shell line in words", () => {
  const n = versionNotes({
    latest: "v3.63.0",
    host: "3.62.2",
    dashboard: "3.62.2",
    update_available: true,
    dashboard_older: false,
  });
  assert.match(n.update ?? "", /v3\.63\.0/);
  assert.equal(n.older, null);
  const s = applySummary({
    deploy: ["media", "new1"],
    new: ["new1"],
    unchanged: ["home"],
    destroy: ["drill"],
    ephemeral: [],
    broken: [],
  });
  assert.equal(s.headline, "2 to deploy · 1 unchanged · 1 gone from the files");
  assert.ok(s.pending && !s.blocked);
  assert.ok(s.lines.some((l) => l.includes("new1: new")));
  const blocked = applySummary({
    deploy: [],
    new: [],
    unchanged: [],
    destroy: [],
    ephemeral: [],
    broken: [["media", "latch failed"]],
  });
  assert.match(blocked.blocked, /do not build/);
  assert.deepEqual(
    shellResult({ state: "done", message: "exit 0\n/dev/sda1 30G\n" }),
    {
      done: true,
      exit: 0,
      ok: true,
      output: "/dev/sda1 30G\n",
    },
  );
  assert.deepEqual(
    shellResult({ state: "failed", message: "exec is disabled on this host" }),
    {
      done: true,
      exit: null,
      ok: false,
      output: "exec is disabled on this host",
    },
  );
  assert.equal(shellResult({ state: "running", message: null }).done, false);
});

test("an import is the bundle, the new name and number, held in the wizard's words", async () => {
  const { importBody, importErrors } = await import("../js/editforms.js");
  assert.deepEqual(
    importBody({ bundle: "a: 1\n", name: " recipes2 ", vmid: " 197 " }),
    { bundle: "a: 1\n", name: "recipes2", vmid: 197 },
  );
  const taken = { names: ["media"], vmids: [106] };
  assert.deepEqual(
    Object.keys(importErrors({ bundle: "", name: "", vmid: "" }, taken)).sort(),
    ["bundle", "name", "vmid"],
  );
  const e = importErrors({ bundle: "x", name: "media", vmid: "106" }, taken);
  assert.match(e.name ?? "", /media already/);
  assert.match(e.vmid ?? "", /CT 106 exists/);
  assert.deepEqual(
    importErrors({ bundle: "x", name: "recipes2", vmid: "197" }, taken),
    {},
  );
});

test("a manual check's choice carries its whole text", () => {
  const text = "Open een route van buitenaf op je telefoon. ".repeat(6);
  const [c] = checkChoices([
    { id: "a1", record: { stack: "gateway", app: "cloudflared", text } },
  ]);
  assert.deepEqual(c, { id: "a1", label: `gateway/cloudflared: ${text}` });
});
