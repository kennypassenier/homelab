// Milestone edit: the edit forms as data, the plan and the fleet's
// firewall as pure view models.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  checkFields,
  checkNewStep,
  commitFields,
  dataFields,
  editable,
  fieldText,
  firewallBody,
  firewallChanged,
  firewallModel,
  hostSettingsBody,
  moveRule,
  newStackBody,
  newStackWizard,
  parseKey,
  presetSize,
  ruleFields,
  ruleFromValues,
  ruleProblems,
  ruleSummary,
  settingsBody,
  settingsForm,
  startValues,
  valueText,
} from "../js/editforms.js";
import { editCommands } from "../js/commands.js";
import { matrixRows, ruleRows, stackState } from "../js/fwview.js";
import { commitBody, committedText, planView } from "../js/plan.js";

/** @type {import("../js/editforms.js").ManifestView} */
const manifest = {
  vmid: 116,
  hostname: "116-app-kp-soft",
  ip: "10.10.10.16/24",
  resources: { cores: 2, memory_mb: 2048, swap_mb: 0, disk_gb: 16 },
  boot: { onboot: true, order: 75 },
  protection: true,
  apps: ["kp-soft"],
  natives: [],
  firewall: {
    enabled: true,
    comment: "top",
    policy_in: "DROP",
    policy_out: "ACCEPT",
    management_open: null,
    rules: [
      { dir: "out", action: "DROP", dest: "10.10.10.250" },
      {
        dir: "in",
        action: "ACCEPT",
        source: "10.10.10.4",
        proto: "tcp",
        dport: "8080,8787",
      },
    ],
  },
};

test("the settings form sends only what changed, with stable field ids", () => {
  const form = settingsForm("kp-soft", manifest, {
    "kp-soft/kp-soft": "ghcr.io/k/kp-soft:1.0",
  });
  const ids = form.steps[0].fields.map((f) => f.id);
  assert.ok(ids.includes("edit-memory-mb"));
  assert.ok(ids.includes("edit-image-kp-soft-kp-soft"));
  const v = startValues(form);
  assert.deepEqual(settingsBody(form, v), { kind: "settings" });
  v.memory_mb = "3072";
  v.onboot = false;
  v["image:kp-soft/kp-soft"] = "ghcr.io/k/kp-soft:1.1 ";
  assert.deepEqual(settingsBody(form, v), {
    kind: "settings",
    memory_mb: 3072,
    onboot: false,
    images: { "kp-soft/kp-soft": "ghcr.io/k/kp-soft:1.1" },
  });
  v.cores = "0";
  v.disk_gb = "abc";
  const e = checkFields(form, v);
  assert.match(e.cores, /from 1 to 64/);
  assert.match(e.disk_gb, /whole number/);
});

test("the firewall model keeps origins, and moving or adding shows as a change", () => {
  const m = firewallModel(manifest.firewall);
  assert.equal(firewallChanged(m, manifest.firewall), false);
  assert.deepEqual(
    m.rules.map((r) => r.origin),
    [0, 1],
  );
  const moved = moveRule(m, 1, -1);
  assert.deepEqual(
    moved.rules.map((r) => r.origin),
    [1, 0],
  );
  assert.equal(firewallChanged(moved, manifest.firewall), true);
  assert.equal(moveRule(m, 0, -1), m);
  const body = firewallBody({
    ...m,
    rules: [
      ...m.rules,
      {
        origin: null,
        rule: ruleFromValues({
          dir: "in",
          action: "ACCEPT",
          peer: " 10.10.10.7 ",
          proto: "icmp",
          dport: "",
          note: "",
          comment: "",
        }),
      },
    ],
  });
  assert.deepEqual(body.rules[2], {
    origin: null,
    rule: { dir: "in", action: "ACCEPT", source: "10.10.10.7", proto: "icmp" },
  });
  assert.equal(body.comment, "top");
  assert.equal(body.management_open, null);
  // A stack without a firewall starts closed and off.
  const none = firewallModel(null);
  assert.deepEqual(
    [none.enabled, none.policy_in, none.policy_out],
    [false, "DROP", "ACCEPT"],
  );
});

test("a rule reads in words, and its form refuses what Proxmox would misread", () => {
  assert.equal(
    ruleSummary(manifest.firewall?.rules[1] ?? { dir: "in", action: "DROP" }),
    "IN ACCEPT from 10.10.10.4 tcp port 8080,8787",
  );
  assert.equal(
    ruleSummary({ dir: "out", action: "DROP" }),
    "OUT DROP to anywhere",
  );
  const f = ruleFields(null);
  assert.deepEqual(
    f.map((x) => x.id),
    [
      "rule-dir",
      "rule-action",
      "rule-peer",
      "rule-proto",
      "rule-dport",
      "rule-note",
      "rule-comment",
    ],
  );
  assert.match(ruleProblems({ peer: "10.10.10.4/24" }).peer, /host bits/);
  assert.equal(ruleProblems({ peer: "10.10.10.0/24" }).peer, undefined);
  assert.match(ruleProblems({ peer: "10.10.10.400" }).peer, /not an address/);
  assert.match(ruleProblems({ proto: "", dport: "80" }).dport, /tcp or udp/);
  assert.match(ruleProblems({ proto: "icmp", dport: "80" }).dport, /no ports/);
  assert.match(
    ruleProblems({ proto: "tcp", dport: "90:80" }).dport,
    /forward range/,
  );
  assert.deepEqual(
    ruleProblems({ proto: "tcp", dport: "5000:5003, 8080" }),
    {},
  );
  assert.match(ruleProblems({ note: "a\nb" }).note, /one line/);
});

test("the new-stack wizard is data: steps, ids, the preset's size, the body", () => {
  const presets = [
    {
      name: "mealie",
      description: "Recipes",
      ram_mb: 512,
      cores: 2,
      disk_gb: 32,
      apps: ["mealie"],
      gpu: false,
      vpn: false,
    },
    {
      name: "custom",
      description: "Empty",
      ram_mb: 1024,
      cores: 2,
      disk_gb: 32,
      apps: [],
      gpu: false,
      vpn: false,
    },
  ];
  const w = newStackWizard(presets, 121);
  assert.deepEqual(
    w.steps.map((s) => s.id),
    ["preset", "identity", "size", "data", "plan"],
  );
  assert.deepEqual(
    w.steps.flatMap((s) => s.fields.map((f) => f.id)),
    [
      "new-preset",
      "new-name",
      "new-vmid",
      "new-ram-mb",
      "new-cores",
      "new-disk-gb",
      "new-swap-mb",
    ],
  );
  const v = startValues(w);
  assert.equal(v.vmid, "121");
  assert.equal(v.ram_mb, "512");
  assert.deepEqual(presetSize(presets[1]), {
    ram_mb: "1024",
    cores: "2",
    disk_gb: "32",
  });
  v.name = "kp-soft";
  const taken = { names: ["kp-soft"], vmids: [116] };
  assert.match(checkNewStep(w, "identity", v, taken).name, /already/);
  v.name = "Recipes";
  assert.match(checkNewStep(w, "identity", v, taken).name, /valid/);
  v.name = "recipes";
  v.vmid = "101";
  assert.match(checkNewStep(w, "identity", v, taken).vmid, /from 102/);
  v.vmid = "121";
  assert.deepEqual(checkNewStep(w, "identity", v, taken), {});
  const data = dataFields(["/appdata/recipes/mealie-config"]);
  assert.equal(data[0].id, "new-nodata-0");
  v[data[0].name] = true;
  assert.deepEqual(newStackBody(v), {
    name: "recipes",
    vmid: 121,
    preset: "mealie",
    ram_mb: 512,
    cores: 2,
    disk_gb: 32,
    no_data: ["/appdata/recipes/mealie-config"],
  });
  v.swap_mb = "256";
  assert.equal(newStackBody(v).swap_mb, 256);
});

test("the commit step offers what the plan says can follow", () => {
  const f = commitFields(["deploy", "resize"], "stacks/x: y [feat-stacks-2]");
  const follow = f.find((x) => x.name === "follow");
  assert.deepEqual(
    follow?.choices?.map((c) => c.value),
    ["none", "deploy", "resize"],
  );
  assert.equal(follow?.current, "deploy");
  assert.equal(
    commitFields([], "s").find((x) => x.name === "follow")?.current,
    "none",
  );
  assert.deepEqual(
    commitBody({ kind: "raw" }, { subject: " s ", note: "", follow: "none" }),
    {
      edit: { kind: "raw" },
      subject: "s",
    },
  );
  assert.deepEqual(
    commitBody({ kind: "raw" }, { subject: "", follow: "deploy" }),
    {
      edit: { kind: "raw" },
      follow: "deploy",
    },
  );
});

test("the plan reads as files, effects and whether it may go", () => {
  /** @type {import("../js/plan.js").Plan} */
  const plan = {
    stack: "kp-soft",
    kind: "settings",
    head: "abc",
    sync_error: null,
    files: [
      {
        path: "stacks/kp-soft/lxc-compose.yml",
        status: "changed",
        added: 1,
        removed: 1,
        hunks: [
          {
            old_start: 33,
            old_len: 2,
            new_start: 33,
            new_len: 2,
            lines: [
              { op: "=", old: 33, new: 33, text: "  cores: 2" },
              { op: "-", old: 34, new: null, text: "  memory_mb: 2048" },
              { op: "+", old: null, new: 34, text: "  memory_mb: 3072" },
            ],
          },
        ],
      },
    ],
    effects: [
      {
        tone: "info",
        what: "resize applies it",
        detail: ["memory 2 GB → 3 GB"],
        by: "resize",
      },
    ],
    follow_ups: ["resize"],
    problems: [],
    valid: true,
    unchanged: false,
    applied: { changes: ["~ kp-soft/docker-compose.yml"] },
    subject: "s",
    restarts_dashboard: false,
  };
  const v = planView(plan);
  assert.deepEqual(
    v.files[0].hunks[0].lines.map((l) => [l.cls, l.no]),
    [
      ["diff-same", 33],
      ["diff-del", 34],
      ["diff-add", 34],
    ],
  );
  assert.equal(v.files[0].hunks[0].head, "@@ −33,2 +33,2 @@");
  assert.equal(v.effects[0].what, "Resize applies it");
  assert.equal(v.effects[0].by, "by resize");
  assert.match(v.applied, /change 1 file/);
  assert.equal(v.blocked, false);
  const bad = planView({ ...plan, valid: false, problems: ["vmid taken"] });
  assert.equal(bad.blocked, true);
  assert.match(bad.blockedWhy, /refuses/);
  assert.match(
    planView({ ...plan, valid: false, unchanged: true, files: [] }).blockedWhy,
    /Nothing changes/,
  );
  assert.match(
    planView({ ...plan, applied: { never: true, changes: [] } }).applied,
    /never applied/,
  );
  assert.equal(
    committedText({
      committed: {
        commit: "0123456789abcdef",
        subject: "s [x-1]",
        pushed: true,
        landed_despite_error: false,
      },
      follow: { job: 7, action: "deploy-commit" },
    }),
    "Committed 0123456789 and pushed: s [x-1]. Deploy queued as job 7.",
  );
});

test("the fleet's firewall: states, rules with peers named, the matrix", () => {
  assert.equal(
    stackState({
      stack: "a",
      vmid: 1,
      ip: "",
      declared: false,
      enabled: false,
      policy_in: null,
      policy_out: null,
      rules: 0,
      management_open: null,
    }).label,
    "none declared",
  );
  const rows = ruleRows([
    {
      stack: "admin",
      vmid: 120,
      n: 5,
      dir: "in",
      action: "ACCEPT",
      peer: "10.10.10.4",
      peer_stacks: ["gateway"],
      proto: "tcp",
      ports: "8090",
      note: "Traefik",
      enabled: true,
    },
    {
      stack: "admin",
      vmid: 120,
      n: 2,
      dir: "out",
      action: "DROP",
      peer: "10.10.10.250",
      peer_stacks: [],
      proto: "any",
      ports: "",
      note: "",
      enabled: true,
    },
  ]);
  assert.equal(rows[0].peer, "10.10.10.4 (gateway)");
  assert.equal(rows[1].ports, "any");
  assert.equal(rows[1].tone, "bad");
  const m = matrixRows({
    stacks: ["admin", "gateway", "home"],
    rules: [],
    unguarded: ["home"],
    cells: [
      {
        from: "gateway",
        to: "admin",
        state: "some",
        allowed: ["tcp 8090"],
        stopped_at_source: [],
      },
      {
        from: "admin",
        to: "gateway",
        state: "none",
        allowed: [],
        stopped_at_source: ["tcp 80"],
      },
      {
        from: "admin",
        to: "home",
        state: "open",
        allowed: ["everything"],
        stopped_at_source: [],
      },
      {
        from: "gateway",
        to: "home",
        state: "open",
        allowed: ["everything"],
        stopped_at_source: [],
      },
      {
        from: "home",
        to: "admin",
        state: "none",
        allowed: [],
        stopped_at_source: [],
      },
      {
        from: "home",
        to: "gateway",
        state: "none",
        allowed: [],
        stopped_at_source: [],
      },
    ],
  });
  assert.deepEqual(
    m[0].cells.map((c) => c.text),
    ["—", "stopped", "open"],
  );
  assert.deepEqual(
    m[1].cells.map((c) => c.text),
    ["tcp 8090", "—", "open"],
  );
  assert.match(m[0].cells[1].title, /admin stops: tcp 80/);
});

test("host settings: values read as a person reads them, typed text becomes JSON", () => {
  /** @type {import("../js/editforms.js").HostField} */
  const hour = {
    key: "backup_hour",
    group: "g",
    label: "Nightly hour",
    help: "",
    default: "off",
    kind: { type: "int", min: 0, max: 23 },
    access: "browser",
    apply: "live",
    set: true,
    value: 4,
    toml: null,
  };
  assert.equal(valueText(hour), "4");
  assert.equal(valueText({ ...hour, set: false }), "default: off");
  assert.equal(
    valueText({
      ...hour,
      key: "ask_timeout_s",
      kind: { type: "int", min: 1, max: 99999 },
      value: 7200,
    }),
    "2 h",
  );
  assert.equal(
    valueText({ ...hour, access: "secret", value: null }),
    "set (hidden)",
  );
  assert.equal(
    valueText({ ...hour, kind: { type: "table" }, value: [{}, {}] }),
    "2 entries",
  );
  assert.equal(
    fieldText({ ...hour, kind: { type: "table" }, toml: "[[retention]]\n" }),
    "[[retention]]\n",
  );
  assert.equal(editable({ ...hour, access: "locked" }), false);
  assert.equal(editable({ ...hour, access: "confirm" }), true);
  assert.deepEqual(parseKey(hour, "5"), { ok: true, value: 5 });
  assert.equal(parseKey(hour, "24").ok, false);
  assert.deepEqual(parseKey(hour, " "), { ok: true, value: null });
  assert.deepEqual(
    parseKey({ ...hour, kind: { type: "vmid_list" } }, "104, 105"),
    { ok: true, value: [104, 105] },
  );
  assert.equal(
    parseKey({ ...hour, kind: { type: "url" } }, "ftp://x").ok,
    false,
  );
  assert.equal(parseKey({ ...hour, kind: { type: "window" } }, "24h").ok, true);
  assert.deepEqual(parseKey({ ...hour, kind: { type: "bool" } }, true), {
    ok: true,
    value: true,
  });
  const table = {
    ...hour,
    key: "retention",
    kind: /** @type {const} */ ({ type: "table" }),
  };
  const staged = new Map([
    ["backup_hour", { field: hour, value: 5 }],
    ["retention", { field: table, value: "[[retention]]\nevery_days = 1\n" }],
  ]);
  assert.deepEqual(
    hostSettingsBody(
      "f".repeat(64),
      staged,
      new Set(["gateway_vmid", "retention"]),
    ),
    {
      expect_sha256: "f".repeat(64),
      values: { backup_hour: 5 },
      fragments: { retention: "[[retention]]\nevery_days = 1\n" },
      confirms: ["retention"],
    },
  );
});

test("every stack's edit tabs and a new stack are palette commands, the open stack first", () => {
  const fleet = /** @type {any} */ ({
    stacks: [{ name: "gateway" }, { name: "media" }],
  });
  let opened = 0;
  const list = editCommands(
    () => "media",
    () => (opened += 1),
  )({ fleet, themes: [], theme: null });
  assert.deepEqual(
    list.map((c) => c.id),
    [
      "edit:new-stack",
      "edit:settings:media",
      "edit:firewall:media",
      "edit:settings:gateway",
      "edit:firewall:gateway",
    ],
  );
  assert.equal(list[2].href, "/app/stacks/media/firewall");
  list[0].run?.();
  assert.equal(opened, 1);
});
