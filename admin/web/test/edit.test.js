// Milestone edit: the edit forms as data, the plan and the fleet's
// firewall as pure view models.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  addAppBody,
  appsAddBlankField,
  appsBody,
  appsRemoveField,
  checkFields,
  checkNewStep,
  checkRowFields,
  checkRowFromValues,
  checkRowProblems,
  checkRowSummary,
  checksBody,
  commitFields,
  dataFields,
  dataMountFields,
  dataMountFromValues,
  dataMountProblems,
  dataMountSummary,
  editable,
  fieldText,
  firewallBody,
  firewallChanged,
  firewallModel,
  hostSettingsBody,
  latchBody,
  latchFileFields,
  latchFileFromValues,
  latchFileProblems,
  latchFileSummary,
  latchSecretField,
  latchSecretsProblems,
  logFileFields,
  logFileFromValues,
  logFileSummary,
  manualRow,
  manualRowFields,
  manualRowFromValues,
  manualRowSummary,
  moveRule,
  newStackBody,
  newStackWizard,
  parseKey,
  presetSize,
  probeRowFields,
  probeRowFromValues,
  probeRowProblems,
  probeRowSummary,
  retentionRowFields,
  retentionRowFromValues,
  retentionRowProblems,
  retentionRowSummary,
  rowBody,
  rowModel,
  rowsChanged,
  ruleFields,
  ruleFromValues,
  ruleProblems,
  ruleSummary,
  settingsBody,
  settingsExtBody,
  settingsExtForm,
  settingsForm,
  startValues,
  storageFields,
  storageFromValues,
  storageSummary,
  tileProblems,
  tileRowFields,
  tileRowFromValues,
  tileRowProblems,
  tileRowSummary,
  tilesBody,
  tilesEditBody,
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

// owner remark 2026-09-30 ("de uptime-check tijd … in de wizard"): the
// settings form's per-tile watch fields, blank by default (fleet default
// applies), writing into `tiles.<key>.{watch_every,down_after}`.
test("each tile the stack declares gets two optional watch fields", () => {
  /** @type {import("../js/editforms.js").ManifestView} */
  const withTile = {
    ...manifest,
    tiles: {
      "kuma.kp-soft.dev": {
        name: "Uptime Kuma",
        group: "Infrastructure",
        watch_every: 60,
        down_after: 300,
      },
    },
  };
  const form = settingsForm("uptime", withTile, {});
  const ids = form.steps[0].fields.map((f) => f.id);
  assert.ok(ids.includes("edit-tile-watch-kuma-kp-soft-dev"));
  assert.ok(ids.includes("edit-tile-down-kuma-kp-soft-dev"));
  const v = startValues(form);
  assert.equal(v["tile_watch_every:kuma.kp-soft.dev"], "60");
  assert.equal(v["tile_down_after:kuma.kp-soft.dev"], "300");
  // Unchanged: nothing sent.
  assert.deepEqual(settingsBody(form, v), { kind: "settings" });
  v["tile_watch_every:kuma.kp-soft.dev"] = "30";
  v["tile_down_after:kuma.kp-soft.dev"] = "180";
  assert.deepEqual(settingsBody(form, v), {
    kind: "settings",
    tiles: { "kuma.kp-soft.dev": { watch_every: 30, down_after: 180 } },
  });
  // A stack without tiles gets no fields at all: there is nothing to
  // create one from in this wizard (tiles are hand-declared in the stack
  // file, not built by the new-stack or add-app wizard).
  assert.equal(
    settingsForm("kp-soft", manifest, {}).steps[0].fields.some((f) =>
      f.name.startsWith("tile_watch_every:"),
    ),
    false,
  );
});

test("down after must be at least check every, across the two tile fields", () => {
  assert.deepEqual(
    tileProblems({
      "tile_watch_every:kuma.kp-soft.dev": "120",
      "tile_down_after:kuma.kp-soft.dev": "60",
    }),
    {
      "tile_down_after:kuma.kp-soft.dev":
        "kuma.kp-soft.dev: down after (60 s) must be at least check every (120 s).",
    },
  );
  // One side blank: nothing to compare yet (the fleet default fills it).
  assert.deepEqual(
    tileProblems({
      "tile_watch_every:kuma.kp-soft.dev": "120",
      "tile_down_after:kuma.kp-soft.dev": "",
    }),
    {},
  );
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
    ["preset", "identity", "size", "data", "tile", "plan"],
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
      "new-tile-hostname",
      "new-tile-name",
      "new-tile-group",
      "new-tile-description",
      "new-tile-watch-every",
      "new-tile-down-after",
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

test("the wizard's Tile step folds into newStackBody's own commit, not a second one", () => {
  const v = /** @type {any} */ ({
    name: "recipes",
    vmid: "121",
    preset: "mealie",
    ram_mb: "512",
    cores: "2",
    disk_gb: "32",
  });
  // Blank hostname: no tile at all, not an empty one.
  assert.equal("tile" in newStackBody(v), false);
  v.tile_hostname = "recipes.kp-soft.dev";
  v.tile_watch_every = "60";
  assert.deepEqual(newStackBody(v).tile, {
    hostname: "recipes.kp-soft.dev",
    name: "recipes",
    group: "Own",
    watch_every: 60,
  });
  v.tile_name = "Recipes";
  v.tile_group = "Household";
  v.tile_description = "Meal planning";
  v.tile_down_after = "300";
  assert.deepEqual(newStackBody(v).tile, {
    hostname: "recipes.kp-soft.dev",
    name: "Recipes",
    group: "Household",
    description: "Meal planning",
    watch_every: 60,
    down_after: 300,
  });
});

test("addAppBody folds each app's optional tile into the app's own commit", () => {
  assert.deepEqual(addAppBody("mealie", {}), {
    kind: "add_app",
    preset: "mealie",
  });
  assert.deepEqual(addAppBody("mealie", { mealie: "recipes.kp-soft.dev" }), {
    kind: "add_app",
    preset: "mealie",
    tiles: { mealie: "recipes.kp-soft.dev" },
  });
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
    v.files[0].hunks[0].lines.map((l) => [l.kind, l.no]),
    [
      ["same", 33],
      ["removed", 34],
      ["added", 34],
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
  assert.equal(list[2].href, "/stacks/media/firewall");
  list[0].run?.();
  assert.equal(opened, 1);
});

test("the settings-ext form sends only what changed; retention is a row table, not JSON", () => {
  const m = /** @type {any} */ ({
    ...manifest,
    network: { ip: "10.10.10.16/24", gateway: "10.10.10.1", bridge: "vmbr0" },
    lxc: { unprivileged: true, gpu: false, vpn: false },
    resources: { ...manifest.resources, storage: "local-lvm" },
    on_demand: false,
    retention: null,
  });
  const form = settingsExtForm("kp-soft", m);
  assert.deepEqual(
    form.steps[0].fields.map((f) => f.id),
    [
      "edit-network-ip",
      "edit-network-gateway",
      "edit-network-bridge",
      "edit-network-vlan",
      "edit-lxc-unprivileged",
      "edit-lxc-gpu",
      "edit-lxc-vpn",
      "edit-lxc-timezone",
      "edit-resources-storage",
      "edit-on-demand",
    ],
  );
  const values = startValues(form);
  assert.deepEqual(settingsExtBody(form, values, null), {
    kind: "settings_ext",
  });
  values.ip = "10.10.10.20/24";
  values.gpu = true;
  assert.deepEqual(settingsExtBody(form, values, null), {
    kind: "settings_ext",
    ip: "10.10.10.20/24",
    gpu: true,
  });
  // W2: retention is the same origin-less full-list shape the backend
  // already took for the textarea — a plain array, no `origin` field, an
  // empty list going back to the fleet default.
  assert.deepEqual(
    settingsExtBody(form, values, [
      { origin: 0, row: { every_days: 1, span_days: 7 } },
      { origin: null, row: { every_days: 30 } },
    ]).retention,
    [{ every_days: 1, span_days: 7 }, { every_days: 30 }],
  );
  assert.deepEqual(settingsExtBody(form, values, []).retention, []);
});

test("retentionRowFields/FromValues/Problems/Summary — the retention row dialog", () => {
  assert.deepEqual(
    retentionRowFields({ every_days: 7, span_days: 90 }).map((f) => [
      f.name,
      f.current,
    ]),
    [
      ["every_days", "7"],
      ["span_days", "90"],
    ],
  );
  assert.deepEqual(
    retentionRowFields(null).map((f) => f.current),
    ["", ""],
  );
  assert.deepEqual(
    retentionRowFromValues({ every_days: "1", span_days: "7" }),
    { every_days: 1, span_days: 7 },
  );
  assert.deepEqual(
    retentionRowFromValues({ every_days: "30", span_days: "" }),
    {
      every_days: 30,
    },
  );
  assert.equal(
    retentionRowProblems({ every_days: "10", span_days: "3" }).span_days,
    "Must be at least the keep-one-every span.",
  );
  assert.deepEqual(
    retentionRowProblems({ every_days: "1", span_days: "7" }),
    {},
  );
  assert.equal(
    retentionRowSummary({ every_days: 1, span_days: 7 }),
    "every 1d, kept 7d",
  );
  assert.equal(
    retentionRowSummary({ every_days: 30 }),
    "every 30d, kept forever",
  );
});

test("the origin-tracked row helpers: model, body, change detection", () => {
  const rows = rowModel([{ a: 1 }, { a: 2 }]);
  assert.deepEqual(rows, [
    { origin: 0, row: { a: 1 } },
    { origin: 1, row: { a: 2 } },
  ]);
  assert.deepEqual(rowBody(rows), [
    { origin: 0, a: 1 },
    { origin: 1, a: 2 },
  ]);
  assert.equal(rowsChanged(rows, rows), false);
  const edited = [{ origin: 0, row: { a: 9 } }, rows[1]];
  assert.equal(rowsChanged(edited, rows), true);
});

test("a storage row reads in words and its form has an app picker", () => {
  const m = /** @type {any} */ ({ ...manifest, apps: ["jellyfin", "sonarr"] });
  assert.equal(
    storageSummary({
      host_path: "/appdata/media/jellyfin-config",
      mount_point: "/config",
      app: "jellyfin",
    }),
    "/appdata/media/jellyfin-config → /config (jellyfin)",
  );
  const fields = storageFields(m, null);
  const app = fields.find((f) => f.name === "app");
  assert.deepEqual(
    app?.choices?.map((c) => c.value),
    ["", "jellyfin", "sonarr"],
  );
  assert.deepEqual(
    storageFromValues({
      host_path: "/appdata/media/jellyfin-config",
      mount_point: "/config",
      no_data: true,
      app: "jellyfin",
    }),
    {
      host_path: "/appdata/media/jellyfin-config",
      mount_point: "/config",
      no_data: true,
      app: "jellyfin",
    },
  );
});

test("a data mount's rotate fields: absent files means no rotation, and orphan fields refuse", () => {
  assert.deepEqual(
    dataMountFromValues({ host_path: "/mnt/a", mount_point: "/data" }),
    { host_path: "/mnt/a", mount_point: "/data" },
  );
  assert.deepEqual(
    dataMountFromValues({
      host_path: "/mnt/a",
      mount_point: "/data",
      rotate_files: "access.log",
      rotate_keep: "30",
      rotate_container: "traefik",
      rotate_signal: "USR1",
    }),
    {
      host_path: "/mnt/a",
      mount_point: "/data",
      rotate: {
        files: "access.log",
        keep: 30,
        reopen: { container: "traefik", signal: "USR1" },
      },
    },
  );
  assert.match(
    dataMountProblems({ rotate_keep: "5" }).rotate_keep,
    /file name\/glob above/,
  );
  assert.deepEqual(
    dataMountProblems({ rotate_files: "x.log", rotate_keep: "5" }),
    {},
  );
  assert.equal(
    dataMountSummary({ host_path: "/mnt/a", mount_point: "/data" }),
    "/mnt/a → /data",
  );
  assert.deepEqual(
    dataMountFields(null).map((f) => f.id),
    [
      "mount-host-path",
      "mount-mount-point",
      "mount-note",
      "mount-rotate-files",
      "mount-rotate-keep",
      "mount-rotate-container",
      "mount-rotate-signal",
    ],
  );
});

test("a log_files row is two fields", () => {
  assert.deepEqual(
    logFileFields(null).map((f) => f.id),
    ["logfile-path", "logfile-job"],
  );
  assert.deepEqual(
    logFileFromValues({ path: "/var/log/x.log", job: "access" }),
    {
      path: "/var/log/x.log",
      job: "access",
    },
  );
  assert.equal(
    logFileSummary({ path: "/var/log/x.log", job: "access" }),
    "/var/log/x.log (access)",
  );
});

test("a latch_files row refuses '${' everywhere, and restarts picks a native", () => {
  const m = /** @type {any} */ ({ ...manifest, natives: ["kyu"] });
  const fields = latchFileFields(m, null);
  const restarts = fields.find((f) => f.name === "restarts");
  assert.deepEqual(
    restarts?.choices?.map((c) => c.value),
    ["", "kyu"],
  );
  assert.deepEqual(
    latchFileFromValues({
      from: "a.env",
      dest: "/etc/a.env",
      mode: "640",
      restarts: "kyu",
    }),
    { from: "a.env", dest: "/etc/a.env", mode: "640", restarts: "kyu" },
  );
  assert.match(latchFileProblems({ dest: "/etc/${X}" }).dest, /latch --expand/);
  assert.deepEqual(
    latchFileProblems({ from: "a.env", dest: "/etc/a.env", mode: "640" }),
    {},
  );
  assert.equal(
    latchFileSummary({ from: "a.env", dest: "/etc/a.env" }),
    "a.env → /etc/a.env",
  );
});

test("apps_remove and latch_secret are one field per app, from the same template", () => {
  const remove = appsRemoveField("jellyfin");
  assert.equal(remove.id, "apps-remove-jellyfin");
  assert.equal(remove.name, "apps-remove:jellyfin");
  const secret = latchSecretField("jellyfin");
  assert.equal(secret.id, "latch-secret-jellyfin");
  assert.equal(appsAddBlankField().id, "apps-add-blank");
  assert.deepEqual(latchSecretsProblems(["ok-app"]), {});
  assert.match(
    latchSecretsProblems(["bad${app"])["latch-secret:bad${app"],
    /latch --expand/,
  );
});

test("appsBody and latchBody send only the parts touched", () => {
  assert.deepEqual(
    appsBody({
      remove: [],
      addBlank: [],
      storage: null,
      dataMounts: null,
      logFiles: null,
    }),
    { kind: "apps" },
  );
  assert.deepEqual(
    appsBody({
      remove: ["old-app"],
      addBlank: ["new-app"],
      storage: rowModel([
        { host_path: "/appdata/x/y-config", mount_point: "/y" },
      ]),
      dataMounts: null,
      logFiles: null,
    }),
    {
      kind: "apps",
      remove: ["old-app"],
      add_blank: ["new-app"],
      storage: [
        { origin: 0, host_path: "/appdata/x/y-config", mount_point: "/y" },
      ],
    },
  );
  assert.deepEqual(latchBody({ secrets: null, files: null }), {
    kind: "latch",
  });
  assert.deepEqual(
    latchBody({
      secrets: ["jellyfin"],
      files: rowModel([{ from: "a", dest: "/b", mode: "640" }]),
    }),
    {
      kind: "latch",
      secrets: ["jellyfin"],
      files: [{ origin: 0, from: "a", dest: "/b", mode: "640" }],
    },
  );
});

test("a check row reads in words and refuses a missing blind spot below Application", () => {
  assert.deepEqual(
    checkRowFields(null).map((f) => f.id),
    [
      "check-name",
      "check-command",
      "check-expect",
      "check-layer",
      "check-blind-spot",
    ],
  );
  assert.deepEqual(
    checkRowFromValues({
      name: "films",
      command: "echo 3",
      expect: "must_match",
      layer: "network",
    }),
    {
      name: "films",
      command: "echo 3",
      expect: "must_match",
      layer: "network",
    },
  );
  assert.match(checkRowProblems({ layer: "network" }).blind_spot, /blind spot/);
  assert.deepEqual(checkRowProblems({ layer: "application" }), {});
  assert.equal(
    checkRowSummary({ name: "films", layer: "network" }),
    "films (network)",
  );
});

test("a probe row's healthy field round-trips equals/at_least/at_most", () => {
  const fields = probeRowFields({
    name: "x",
    command: "y",
    healthy: { at_least: 3 },
    layer: "network",
  });
  const kind = fields.find((f) => f.name === "healthy_kind");
  const value = fields.find((f) => f.name === "healthy_value");
  assert.equal(kind?.current, "at_least");
  assert.equal(value?.current, "3");
  assert.deepEqual(
    probeRowFromValues({
      name: "films",
      command: "echo 3",
      healthy_kind: "at_least",
      healthy_value: "3",
      layer: "network",
    }),
    {
      name: "films",
      command: "echo 3",
      healthy: { at_least: 3 },
      layer: "network",
    },
  );
  assert.match(
    probeRowProblems({ healthy_kind: "at_least", healthy_value: "abc" })
      .healthy_value,
    /whole number/,
  );
  assert.equal(
    probeRowSummary({ name: "films", healthy: { at_least: 3 } }),
    "films (>= 3)",
  );
});

test("a manual row is a bare string unless once/id/replaces is set, and normalises either way", () => {
  assert.deepEqual(
    manualRowFromValues({ text: "did you register a passkey?", once: true }),
    {
      text: "did you register a passkey?",
      once: true,
    },
  );
  assert.deepEqual(manualRow("plain question"), {
    text: "plain question",
    once: false,
    id: "",
    replaces: "",
  });
  assert.equal(manualRowSummary(manualRow("plain question")), "plain question");
  assert.equal(
    manualRowSummary({ text: "once only", once: true }),
    "once only (once)",
  );
  assert.deepEqual(
    manualRowFields(manualRow("plain question")).map((f) => f.current),
    ["plain question", false, "", ""],
  );
});

test("a manual row's id and replaces round-trip through the dialog fields", () => {
  // fix-182-dashboard-edits: the whole point is that opening and saving
  // the dialog again must not drop the check's identity.
  const loaded = manualRow({
    text: "kijk of het werkt",
    once: false,
    id: "jellyfin-kijk-of-het-werkt",
    replaces: ["abc123", "def456"],
  });
  assert.deepEqual(loaded, {
    text: "kijk of het werkt",
    once: false,
    id: "jellyfin-kijk-of-het-werkt",
    replaces: "abc123,def456",
  });
  const fields = manualRowFields(loaded);
  const byName = Object.fromEntries(fields.map((f) => [f.name, f]));
  assert.equal(byName.id.current, "jellyfin-kijk-of-het-werkt");
  assert.equal(byName.id.readonly, true);
  assert.equal(byName.replaces.current, "abc123,def456");
  assert.equal(byName.replaces.hidden, true);
  const values = Object.fromEntries(
    fields.map((f) => [f.name, f.current ?? ""]),
  );
  assert.deepEqual(manualRowFromValues(values), {
    text: "kijk of het werkt",
    once: false,
    id: "jellyfin-kijk-of-het-werkt",
    replaces: ["abc123", "def456"],
  });
});

test("a manual row with no id/replaces yet does not manufacture any", () => {
  assert.deepEqual(
    manualRowFromValues({
      text: "kijk of het werkt",
      once: false,
      id: "",
      replaces: "",
    }),
    { text: "kijk of het werkt", once: false },
  );
});

test("checksBody sends only the parts touched", () => {
  assert.deepEqual(
    checksBody({
      app: "kp-soft",
      checks: null,
      manual: null,
      probes: null,
      busyCheck: "",
      url: "",
    }),
    { kind: "checks", app: "kp-soft", busy_check: null, url: null },
  );
  assert.deepEqual(
    checksBody({
      app: "kp-soft",
      checks: rowModel([
        { name: "a", command: "b", expect: "must_match", layer: "network" },
      ]),
      manual: null,
      probes: null,
      busyCheck: "echo busy",
      url: "",
    }),
    {
      kind: "checks",
      app: "kp-soft",
      checks: [
        {
          origin: 0,
          name: "a",
          command: "b",
          expect: "must_match",
          layer: "network",
        },
      ],
      busy_check: "echo busy",
      url: null,
    },
  );
});

test("a tile row's fields and the sparse tilesBody (delete needs a tombstone)", () => {
  assert.deepEqual(
    tileRowFields(["Own"], null).map((f) => f.id),
    [
      "tile-key",
      "tile-name",
      "tile-group",
      "tile-order",
      "tile-description",
      "tile-url",
      "tile-watch-url",
      "tile-reading",
      "tile-watch-every",
      "tile-down-after",
    ],
  );
  assert.deepEqual(
    tileRowFromValues({ key: "a.kp-soft.dev", name: "A", group: "Own" }),
    { key: "a.kp-soft.dev", name: "A", group: "Own" },
  );
  assert.match(
    tileRowProblems({ watch_every: "60", down_after: "10" }).down_after,
    /at least check every/,
  );
  assert.equal(
    tileRowSummary({ key: "a.kp-soft.dev", name: "A", group: "Own" }),
    "a.kp-soft.dev · A (Own)",
  );
  const originals = {
    "a.kp-soft.dev": { name: "A", group: "Own" },
    "b.kp-soft.dev": { name: "B", group: "Own" },
  };
  // Untouched: nothing sent.
  const untouched = [
    {
      origin: "a.kp-soft.dev",
      row: { key: "a.kp-soft.dev", name: "A", group: "Own" },
    },
    {
      origin: "b.kp-soft.dev",
      row: { key: "b.kp-soft.dev", name: "B", group: "Own" },
    },
  ];
  assert.deepEqual(tilesBody(untouched, originals), []);
  // "b" deleted (row simply removed from the table) needs its own
  // tombstone, not silent omission.
  const oneDeleted = [untouched[0]];
  assert.deepEqual(tilesBody(oneDeleted, originals), [
    { origin: "b.kp-soft.dev", key: "b.kp-soft.dev", delete: true, tile: {} },
  ]);
  // A brand-new row (origin null) needs no tombstone when later removed
  // again — it never existed in the file.
  const addedThenRemoved = untouched;
  assert.deepEqual(tilesBody(addedThenRemoved, originals), []);
  assert.deepEqual(tilesEditBody(oneDeleted, originals), {
    kind: "tiles",
    tiles: [
      { origin: "b.kp-soft.dev", key: "b.kp-soft.dev", delete: true, tile: {} },
    ],
  });
});
