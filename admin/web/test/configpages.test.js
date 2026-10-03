// redesign-config (3.71.0, Kenny's approved demos firewall.html,
// settings.html, presets.html): the pure halves of the three Configure-side
// pages — the Firewall page's words, numbers and matrix logic, the
// Settings page's filters and counts, the Presets gallery's order, and
// the shared sortable header's click cycle.
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  cellLabel,
  cellWords,
  defaultCell,
  fwAttention,
  fwKpis,
  fwState,
  fwSummary,
  portsText,
  ruleMatches,
  rulesFor,
} from "../js/fwview.js";
import {
  countText,
  emptyPreset,
  galleryCards,
  ramText,
  sets,
} from "../js/presetsview.js";
import {
  accessWords,
  fieldShown,
  foundText,
  groupChip,
  groupsOf,
  repoState,
  sectionTarget,
  shortRemote,
  slug,
  stagedWords,
} from "../js/settingsview.js";
import { applySort, nextSort } from "../js/pages/configkit.js";

/** @param {Partial<import("../js/fwview.js").StackFw>} o */
const stack = (o) => ({
  stack: "x",
  vmid: 100,
  ip: "10.0.0.1",
  declared: true,
  enabled: true,
  policy_in: "DROP",
  policy_out: "ACCEPT",
  rules: 1,
  management_open: null,
  ...o,
});

/** @param {Partial<import("../js/fwview.js").RuleRow>} o */
const rule = (o) => ({
  stack: "gateway",
  vmid: 104,
  n: 1,
  dir: /** @type {"in" | "out"} */ ("out"),
  action: "ACCEPT",
  peer: "10.10.10.16",
  peer_stacks: ["kp-soft"],
  proto: "tcp",
  ports: "8080,8787",
  note: "kp-soft routes",
  enabled: true,
  ...o,
});

/** The demo's own shape: five stacks, two of them unprotected. */
const read = {
  head: { commit: "bb5dce923aa1", subject: "fixture", at: 1 },
  stacks: [
    stack({
      stack: "admin",
      vmid: 120,
      live_enforced: true,
      live_matches_repo: true,
    }),
    stack({
      stack: "alpha-demo",
      vmid: 352,
      declared: false,
      enabled: false,
      policy_in: null,
      policy_out: null,
      rules: 0,
      live_enforced: true,
      live_matches_repo: false,
    }),
    stack({
      stack: "beta-demo",
      vmid: 351,
      declared: false,
      enabled: false,
      policy_in: null,
      policy_out: null,
      rules: 0,
      live_enforced: false,
      live_matches_repo: true,
    }),
    stack({
      stack: "gateway",
      vmid: 104,
      live_enforced: true,
      live_matches_repo: true,
    }),
  ],
  matrix: {
    stacks: ["admin", "alpha-demo", "beta-demo", "gateway"],
    unguarded: [],
    cells: [
      /** @type {import("../js/fwview.js").Cell} */ ({
        from: "gateway",
        to: "admin",
        state: "some",
        allowed: ["tcp 8090"],
        stopped_at_source: [],
      }),
      {
        from: "admin",
        to: "beta-demo",
        state: /** @type {const} */ ("open"),
        allowed: ["everything"],
        stopped_at_source: [],
      },
      {
        from: "gateway",
        to: "beta-demo",
        state: /** @type {const} */ ("some"),
        allowed: ["tcp 8080", "tcp 8787"],
        stopped_at_source: [],
      },
      {
        from: "admin",
        to: "gateway",
        state: /** @type {const} */ ("none"),
        allowed: [],
        stopped_at_source: ["tcp 3100"],
      },
    ],
    rules: [
      rule({ n: 11, peer_stacks: ["beta-demo"] }),
      rule({
        n: 16,
        action: "DROP",
        ports: "",
        proto: "",
        peer: "10.10.10.0/24",
        peer_stacks: ["admin", "beta-demo"],
        note: "every other neighbour",
      }),
      rule({ stack: "beta-demo", dir: "in", n: 6, peer_stacks: ["gateway"] }),
      rule({
        stack: "admin",
        dir: "in",
        n: 5,
        peer_stacks: ["gateway"],
        note: "Traefik",
      }),
    ],
  },
};

test("redesign-firewall: a stack's state in the demo's three words, host over repository", () => {
  const [admin, alpha, beta] = read.stacks;
  assert.deepEqual(fwState(admin), { tone: "ok", word: "in force" });
  assert.deepEqual(fwState(alpha), {
    tone: "warn",
    word: "live differs from files",
  });
  assert.deepEqual(fwState(beta), { tone: "bad", word: "no firewall" });
  assert.deepEqual(fwState(stack({ enabled: false })), {
    tone: "warn",
    word: "declared, switched off",
  });
});

test("redesign-firewall: the KPI strip counts protected, unprotected, open paths and rules by action", () => {
  assert.deepEqual(fwKpis(read), {
    protected: 2,
    total: 4,
    unprotected: 2,
    open: 1,
    rules: 4,
    drop: 1,
    accept: 3,
  });
});

test("redesign-firewall: one attention row per stack not in force, no firewall first, each with its fix", () => {
  const a = fwAttention(read.stacks);
  assert.deepEqual(
    a.map((x) => [x.stack, x.tone, x.fix]),
    [
      ["beta-demo", "bad", "Declare…"],
      ["alpha-demo", "warn", "Compare…"],
    ],
  );
  assert.equal(a[0].title, "beta-demo (CT 351) has no firewall");
  assert.equal(
    a[1].title,
    "alpha-demo: a firewall runs on CT 352, but its stack file declares none",
  );
});

test("redesign-firewall: a square's label, sentence and the rules that decide it", () => {
  const m = read.matrix;
  const c = m.cells[2];
  assert.equal(cellLabel(c), "8080, 8787");
  assert.equal(cellLabel(m.cells[1]), "all");
  assert.equal(cellLabel(m.cells[3]), "");
  assert.equal(cellWords(c), "gateway may open tcp 8080, tcp 8787.");
  assert.match(cellWords(m.cells[1]), /every port/);
  assert.deepEqual(
    rulesFor(m, c).map((r) => `${r.stack} #${r.n} ${r.dir}`),
    ["gateway #11 out", "gateway #16 out", "beta-demo #6 in"],
  );
  // Before a click the square that lets the most ports through is pinned.
  assert.equal(defaultCell(m), c);
});

test("redesign-firewall: the Rules filter takes the actions on (none = all) and plain text", () => {
  const [r11, r16] = read.matrix.rules;
  assert.ok(ruleMatches(r11, new Set(), ""));
  assert.ok(!ruleMatches(r11, new Set(["DROP"]), ""));
  assert.ok(ruleMatches(r16, new Set(["DROP"]), "neighbour"));
  assert.ok(
    ruleMatches(r11, new Set(), "beta-demo"),
    "a peer stack's name matches",
  );
  assert.ok(!ruleMatches(r11, new Set(), "nothing like this"));
  assert.equal(portsText(r11), "tcp 8080, 8787");
  assert.equal(portsText(r16), "any");
});

test("redesign-firewall: one stack's summary for the stack hub's Settings tab", () => {
  const s = fwSummary(read, "gateway");
  assert.ok(s);
  assert.equal(s.word, "in force");
  assert.equal(s.outbound, 2);
  assert.equal(s.inbound, 0);
  assert.deepEqual(
    s.reaches.map((x) => `${x.stack}:${x.label}`),
    ["admin:8090", "beta-demo:8080, 8787"],
  );
  assert.equal(s.href, "/firewall?stack=gateway");
  assert.equal(fwSummary(read, "nope"), null);
});

const presets = [
  {
    name: "mealie",
    description: "Recipes",
    ram_mb: 512,
    cores: 2,
    disk_gb: 32,
    cores_set: false,
    disk_set: false,
    apps: ["mealie"],
  },
  {
    name: "custom",
    description: "Empty stack",
    ram_mb: 1024,
    cores: 2,
    disk_gb: 32,
    apps: [],
  },
  {
    name: "jellyfin",
    description: "Media server (VAAPI)",
    ram_mb: 4096,
    cores: 4,
    disk_gb: 64,
    cores_set: true,
    disk_set: true,
    apps: ["jellyfin"],
    gpu: true,
  },
  {
    name: "actual",
    description: "Envelope budgeting",
    ram_mb: 512,
    cores: 2,
    disk_gb: 32,
    apps: ["actual"],
  },
];

test("redesign-presets: the app-less preset is the Empty stack card, the rest sorted and searched", () => {
  assert.equal(emptyPreset(presets)?.name, "custom");
  assert.deepEqual(
    galleryCards(presets, "", "name").map((p) => p.name),
    ["actual", "jellyfin", "mealie"],
  );
  assert.deepEqual(
    galleryCards(presets, "", "ram").map((p) => p.name),
    ["jellyfin", "actual", "mealie"],
  );
  assert.deepEqual(
    galleryCards(presets, "media", "name").map((p) => p.name),
    ["jellyfin"],
  );
  assert.deepEqual(
    galleryCards(presets, "mealie", "name").length,
    1,
    "an app name matches",
  );
});

test("redesign-presets: sizes and counts in the demo's words", () => {
  assert.equal(ramText(512), "512 MB");
  assert.equal(ramText(1024), "1 GB");
  assert.equal(ramText(4096), "4 GB");
  assert.equal(ramText(1536), "1.5 GB");
  assert.equal(countText(8, 8, "", 107), "8 presets · next free container 107");
  assert.equal(countText(1, 8, "media", 107), "1 of 8 presets");
  assert.deepEqual(sets(presets[0]), { cores: false, disk: false });
  assert.deepEqual(sets(presets[2]), { cores: true, disk: true });
  assert.deepEqual(
    sets(presets[3]),
    { cores: true, disk: true },
    "an older dashboard: shown as set",
  );
});

/** @param {Partial<import("../js/editforms.js").HostField>} o */
const field = (o) =>
  /** @type {import("../js/editforms.js").HostField} */ ({
    key: "backup_hour",
    group: "Nightly round and backups",
    label: "Nightly hour",
    help: "Hour of the nightly backup",
    default: "off",
    kind: { type: "int", min: 0, max: 23 },
    access: "browser",
    apply: "live",
    set: false,
    value: null,
    toml: null,
    ...o,
  });

test("redesign-settings: the search and the Show choice pick the rows", () => {
  const ed = (/** @type {any} */ f) => f.access === "browser";
  const none = () => false;
  const f = field({});
  const locked = field({
    key: "listen",
    label: "Listen address",
    access: "locked",
    set: true,
  });
  assert.ok(fieldShown(f, "nightly", "all", ed, none));
  assert.ok(fieldShown(f, "backup_hour", "all", ed, none), "the key matches");
  assert.ok(!fieldShown(f, "restic", "all", ed, none));
  assert.ok(!fieldShown(f, "", "changed", ed, none), "at its default");
  assert.ok(
    fieldShown(f, "", "changed", ed, () => true),
    "a staged change counts as changed",
  );
  assert.ok(fieldShown(locked, "", "changed", ed, none));
  assert.ok(!fieldShown(locked, "", "here", ed, none));
});

test("redesign-settings: groups, chips, counts and the staged words", () => {
  assert.deepEqual(
    groupsOf([field({}), field({ group: "Logging" }), field({ key: "b" })]),
    ["Nightly round and backups", "Logging"],
  );
  assert.equal(slug("Nightly round and backups"), "nightly-round-and-backups");
  assert.equal(groupChip(17, 17, 2), "17 keys · 2 changed");
  assert.equal(groupChip(3, 17, 0), "3 of 17 keys");
  assert.equal(groupChip(1, 1, 0), "1 key");
  assert.equal(foundText(64, 64, false), "64 settings");
  assert.equal(foundText(5, 64, true), "5 of 64 settings");
  assert.deepEqual(stagedWords(0), {
    chip: "nothing staged",
    write: "Check and write",
  });
  assert.deepEqual(stagedWords(2), {
    chip: "2 changes staged",
    write: "Check and write 2…",
  });
  assert.equal(accessWords(field({}))?.[0] ?? null, null);
  assert.equal(
    accessWords(field({ access: "locked" }))?.[0],
    "ssh only · cuts the dashboard off",
  );
});

test("redesign-settings: the working copy in words, the remote shortened, ?section= found", () => {
  const r = { present: true, behind: 0, unpushed: [], dirty: [], error: null };
  assert.deepEqual(repoState(r), {
    tone: "ok",
    word: "in step with the remote",
  });
  assert.equal(repoState({ ...r, behind: 2 }).tone, "warn");
  assert.equal(repoState({ ...r, dirty: ["a"] }).tone, "bad");
  assert.equal(repoState({ ...r, present: false }).word, "not cloned yet");
  assert.equal(shortRemote("git@github.com:me/stacks.git"), "…/me/stacks.git");
  assert.equal(shortRemote("/tmp/x/fixture-repo"), "…/x/fixture-repo");
  assert.equal(sectionTarget("sign-in", []), "signin");
  assert.equal(sectionTarget("logging", ["Logging"]), "logging");
  assert.equal(sectionTarget("nope", ["Logging"]), null);
  assert.equal(sectionTarget(null, []), null);
});

test("redesign-config: a header click sorts asc → desc → none; Shift adds a second key", () => {
  /** @type {import("../js/pages/configkit.js").SortState} */
  let s = [];
  s = nextSort(s, "stack", false);
  assert.deepEqual(s, [{ key: "stack", dir: 1 }]);
  s = nextSort(s, "stack", false);
  assert.deepEqual(s, [{ key: "stack", dir: -1 }]);
  s = nextSort(s, "stack", false);
  assert.deepEqual(s, []);
  s = nextSort(nextSort(s, "state", false), "rules", true);
  assert.deepEqual(s, [
    { key: "state", dir: 1 },
    { key: "rules", dir: 1 },
  ]);
  const rows = [
    { state: "b", rules: 2 },
    { state: "a", rules: 9 },
    { state: "b", rules: 1 },
  ];
  assert.deepEqual(
    applySort(rows, s, (r, k) => /** @type {any} */ (r)[k]).map((r) => r.rules),
    [9, 1, 2],
  );
});
