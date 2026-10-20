// redesign-kit-4..19 (the senior review of the shared kit, 2026-10-03):
// the shared blocks of ui.js drawn into a small DOM (support/minidom.mjs)
// and read back — the Live view controls they carry, the states they
// switch between, the keyboard — plus the guards over the stylesheets and
// sources the review found duplicated. Each `redesign-kit-N` names its
// REGISTER row.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { install } from "./support/minidom.mjs";

const doc = install();
const ui = await import("../js/ui.js");
const dom = await import("../js/dom.js");
const { declare } = await import("../js/drivable.js");
// Read without failing the whole file where it is missing, so each case
// fails on its own against an older tree (the fail-first run).
const sortstate = /** @type {typeof import("../js/sortstate.js")} */ (
  await import("../js/sortstate.js").catch(() => ({}))
);

for (const [id, row] of [
  ["kit-test-toggle", "a|b|all"],
  ["kit-test-switch", "x|y"],
  ["kit-test-sort", "name|size"],
  ["kit-test-table", "name|size"],
  ["kit-test-undo", undefined],
  ["kit-test-chip", "q"],
  ["kit-test-clear", undefined],
  ["kit-test-retry", undefined],
  ["kit-test-more", undefined],
  ["kit-test-kpi", undefined],
])
  declare({
    id: /** @type {string} */ (id),
    page: "host",
    opens: "view",
    what: "a test control",
    ...(row ? { row } : {}),
  });

const read = (/** @type {string} */ p) =>
  readFileSync(new URL(`../${p}`, import.meta.url), "utf8");
/** @param {any} e */
const pressed = (e) =>
  [...e.querySelectorAll("button")].map(
    (b) =>
      `${b.dataset.value}:${b.getAttribute("aria-pressed") ?? b.getAttribute("aria-checked")}`,
  );

test("redesign-kit-4: a page embedded under another page's title draws its header title as an h2", () => {
  const embedded = ui.pageHeader({
    title: "Schedules",
    desc: "What runs on its own.",
    meta: [],
    level: "h2",
  });
  assert.equal(embedded.title.localName, "h2");
  assert.equal(
    ui.pageHeader({ title: "Host", desc: "The host." }).title.localName,
    "h1",
    "a page of its own keeps its h1",
  );
});

test("redesign-kit-5: a toast is kp-themes' own, in a .kp-toasts region inside #page, its Undo a declared Live view control", async () => {
  let undone = 0;
  const close = ui.toast("Deleted nightly backup", {
    action: {
      label: "Undo",
      run: () => undone++,
      drive: { id: "kit-test-undo" },
    },
  });
  const page = /** @type {any} */ (doc.getElementById("page"));
  const region = page.querySelector(":scope > .kp-toasts");
  assert.ok(region, "no kp-themes toast region inside #page");
  assert.equal(region.getAttribute("aria-live"), "polite");
  const t = region.querySelector(".kp-toast");
  assert.ok(t, "the toast is not kp-themes' .kp-toast");
  assert.ok(t.querySelector(".kp-toast__body"), "kp-themes draws the words");
  const b = t.querySelector("button");
  assert.equal(b.dataset.drive, "kit-test-undo");
  b.click();
  assert.equal(undone, 1, "Undo ran its action");
  // kp-themes 10.0.0: a toast plays its theme's leave before it goes
  // (js/motion.js leave()), so the removal is awaited, not read at once.
  for (let i = 0; i < 50 && region.querySelector(".kp-toast"); i++)
    await new Promise((r) => setTimeout(r, 20));
  assert.equal(region.querySelector(".kp-toast"), null, "Undo closed it");
  ui.toast("plain one", { ms: 0 });
  const last = ui.toast("plain two", { ms: 0 });
  for (
    let i = 0;
    i < 50 && region.querySelectorAll(".kp-toast").length > 1;
    i++
  )
    await new Promise((r) => setTimeout(r, 20));
  assert.equal(
    region.querySelectorAll(".kp-toast").length,
    1,
    "a newer plain toast replaces the older one",
  );
  close();
  last();
  for (const f of [
    "css/app.css",
    "css/pages/secrets.css",
    "js/pages/secrets.js",
  ])
    assert.doesNotMatch(read(f), /sx-toast/, `${f} still restyles the toast`);
});

test("redesign-kit-6: Secrets draws its header and cards with the kit; its sheet keeps no copy of the shared patterns", () => {
  const js = read("js/pages/secrets.js");
  const css = read("css/pages/secrets.css");
  assert.match(js, /pageHeader\(\{/, "Secrets does not use pageHeader");
  // feat-kp11demo-6 (Kenny 2026-10-19): the approved kp-themes 11 demo
  // `app-homelab-settings` draws these panes as kp-card (kp-card__header,
  // __stage), not the old section() head; the page keeps the kit's card.
  assert.match(js, /"kp-card nx-card sx-card"/, "Secrets lost its kp-card");
  assert.match(js, /kp-card__header/, "Secrets lost the kp-card header");
  assert.doesNotMatch(
    js,
    /"nx-header|"nx-card(__head|__foot|__tools)?"|class: "nx-card /,
  );
  for (const shared of [
    ".nx-header",
    ".sx .nx-card {",
    ".sx .nx-card__head {",
    ".sx .nx-card__foot",
    ".sx .nx-chip",
    ".sx .nx-dot",
    ".sx .kp-kbd",
    ".sx .nx-empty",
    ".sx .nx-live {",
    ".sx-tip",
  ])
    assert.ok(!css.includes(shared), `secrets.css still restyles ${shared}`);
});

test("redesign-kit-7: segSwitch moves without a change event; sort state is multi-key, stable and remembered per table", () => {
  /** @type {string[]} */
  const changes = [];
  const sw = ui.segSwitch({
    label: "View",
    items: [
      { value: "x", label: "X" },
      { value: "y", label: "Y" },
    ],
    value: "x",
    onChange: (v) => changes.push(v),
    drive: { id: "kit-test-switch" },
  });
  sw.set("y");
  assert.equal(sw.value(), "y");
  assert.deepEqual(pressed(sw.el), ["x:false", "y:true"]);
  assert.deepEqual(changes, [], "set() is the address moving it, no change");
  /** @type {any} */ (sw.el.querySelector('[data-value="x"]')).click();
  assert.deepEqual(changes, ["x"]);

  const { nextSort, applySort, rememberedSort, keepSort } = sortstate;
  /** @type {{key: string, dir: 1 | -1}[]} */
  let s = nextSort([], "size");
  s = nextSort(s, "name");
  assert.deepEqual(s, [
    { key: "size", dir: 1 },
    { key: "name", dir: 1 },
  ]);
  s = nextSort(s, "name");
  assert.deepEqual(
    s[1],
    { key: "name", dir: -1 },
    "a second click reverses it",
  );
  assert.deepEqual(nextSort(s, "name"), [{ key: "size", dir: 1 }]);
  const rows = [
    { name: "b", size: 2 },
    { name: "a", size: 2 },
    { name: "c", size: null },
    { name: "d", size: 1 },
  ];
  assert.deepEqual(
    applySort(rows, s, (r, k) => r[/** @type {"name" | "size"} */ (k)]).map(
      (r) => r.name,
    ),
    ["d", "b", "a", "c"],
    "size up, then name down; an empty size last",
  );
  assert.deepEqual(
    applySort(
      rows,
      [{ key: "size", dir: -1 }],
      (r, k) => r[/** @type {"size"} */ (k)],
    ).map((r) => r.name),
    ["b", "a", "d", "c"],
    "stable among ties, empty still last when descending",
  );
  keepSort("kit-test", s);
  assert.deepEqual(rememberedSort("kit-test"), s);
  localStorage.setItem("nx-sort:kit-bad", "{oops");
  assert.deepEqual(rememberedSort("kit-bad", [{ key: "a", dir: 1 }]), [
    { key: "a", dir: 1 },
  ]);
});

test("redesign-kit-7: chip, dot, meter, failBox, drawer and keyRow are shared blocks", () => {
  // kp-themes' badge, status dot and meter (kpa-3, kpa-4).
  const c = ui.chip("!", { tone: "warn", label: "unreadable" });
  assert.equal(c.className, "kp-badge kp-badge--warning");
  assert.equal(c.getAttribute("aria-label"), "unreadable");
  assert.equal(ui.chip("3").className, "kp-badge", "no tone: the plain badge");
  assert.equal(
    ui.chip("x", { tone: "bad" }).className,
    "kp-badge kp-badge--destructive",
  );
  assert.equal(
    ui.chip("x", { tone: "ok" }).className,
    "kp-badge kp-badge--success",
  );
  const live = ui.chip("Live", { tone: "live", dot: true });
  assert.equal(live.className, "kp-badge kp-badge--info");
  assert.ok(live.querySelector(".kp-health__dot"), "a dot leads the word");
  const okDot = ui.dot("ok", "running");
  assert.equal(okDot.className, "kp-health");
  assert.equal(okDot.dataset.state, "ok");
  assert.equal(
    okDot.querySelector(".kp-health__dot")?.getAttribute("aria-hidden"),
    "true",
  );
  assert.equal(
    ui.dot("bad", "down").dataset.state,
    "down",
    "kp says down for bad",
  );
  assert.equal(ui.dot("").dataset.state, undefined, "no tone: kp's muted dot");
  assert.equal(ui.dot("bad").getAttribute("aria-hidden"), "true");
  const m = ui.meter({ pct: 140, mark: 46, tone: "bad" });
  assert.equal(m.el.className, "kp-meter");
  assert.equal(m.el.getAttribute("role"), "meter");
  assert.equal(m.el.dataset.kpTone, "destructive");
  assert.equal(m.el.style.getPropertyValue("--kp-value"), "1.4");
  assert.equal(m.el.style.getPropertyValue("--kp-mark"), "0.46");
  assert.equal(m.el.dataset.kpOver, "", "a share past the end says so");
  assert.ok(m.el.querySelector(".kp-meter__mark"), "the tick is kp's mark");
  m.set({ pct: null });
  assert.equal(m.el.hasAttribute("aria-valuenow"), false, "not measured");

  let retried = 0;
  const f = ui.failBox(
    {
      what: "Could not read the repositories",
      why: "the host did not answer",
      fix: "check the host",
    },
    { run: () => retried++, drive: { id: "kit-test-retry" } },
  );
  assert.equal(f.getAttribute("role"), "alert");
  assert.equal(f.dataset.kpState, "error");
  assert.match(f.textContent ?? "", /Fix: check the host/);
  /** @type {any} */ (f.querySelector("button")).click();
  assert.equal(retried, 1);

  let closed = 0;
  const d = ui.drawer({
    title: "New schedule",
    desc: "Read it as a sentence.",
    body: "body",
    onClose: () => closed++,
  });
  d.open();
  assert.ok(d.el.open && d.el.isConnected);
  // kp-themes' drawer: head, body and foot are its parts (kpa-11).
  assert.equal(d.el.className, "kp-drawer kp-dialog");
  assert.ok(d.el.querySelector(".kp-drawer__head > .kp-dialog__title"));
  assert.equal(
    d.el.querySelector(".kp-drawer__desc")?.textContent,
    "Read it as a sentence.",
  );
  assert.equal(d.body.className, "kp-drawer__body");
  const x = /** @type {any} */ (d.el.querySelector('[data-drive="close"]'));
  assert.ok(x, "Close is the dialog control `close`");
  x.click();
  assert.equal(closed, 1);
  assert.equal(d.el.isConnected, false);

  const keys = ui.keyRow([
    [["↑", "↓"], "move"],
    ["/", "search"],
  ]);
  assert.equal(keys.querySelectorAll("kbd").length, 3);
  assert.equal(ui.keysLine, ui.keyRow, "one block, two names");
});

test("redesign-kit-9: one element factory: h skips empty attributes, wires on… handlers and flattens children; el and on are gone", () => {
  let clicks = 0;
  const b = /** @type {any} */ (
    dom.h(
      "button",
      {
        class: "x",
        title: null,
        hidden: false,
        disabled: true,
        onclick: () => clicks++,
      },
      "a",
      null,
      false,
      [1, ["b"]],
    )
  );
  assert.equal(b.hasAttribute("title"), false);
  assert.equal(b.hasAttribute("hidden"), false);
  assert.equal(b.getAttribute("disabled"), "");
  assert.equal(b.textContent, "a1b");
  b.click();
  assert.equal(clicks, 1);
  assert.equal(/** @type {any} */ (dom).el, undefined);
  assert.equal(/** @type {any} */ (dom).on, undefined);
});

test("redesign-kit-10: one toggle group, one rule: drive is required, every value on is All, in both looks", () => {
  assert.throws(
    () =>
      ui.toggleChips(
        /** @type {any} */ ({ label: "Status", chips: [], onChange() {} }),
      ),
    /Live view/,
  );
  for (const look of /** @type {const} */ (["seg", "chips"])) {
    /** @type {Set<string>[]} */
    const seen = [];
    const g = ui.toggleGroup({
      label: "Status",
      look,
      all: { label: "All" },
      chips: [
        { value: "a", label: "A", count: 3 },
        { value: "b", label: "B" },
      ],
      onChange: (on) => seen.push(on),
      drive: { id: "kit-test-toggle" },
    });
    const btn = (/** @type {string} */ v) =>
      /** @type {any} */ (g.el.querySelector(`[data-value="${v}"]`));
    assert.equal(btn("a").dataset.drive, "kit-test-toggle");
    assert.equal(btn("a").dataset.driveRow, "a");
    btn("a").click();
    assert.deepEqual(pressed(g.el), ["all:false", "a:true", "b:false"], look);
    btn("b").click();
    assert.deepEqual(
      pressed(g.el),
      ["all:true", "a:false", "b:false"],
      `${look}: every value on is All`,
    );
    assert.equal(seen.at(-1)?.size, 0);
    g.counts({ a: 7, b: 0, all: 7 });
    assert.equal(btn("a").textContent, "A7", `${look}: the exact count`);
    assert.equal(btn("b").textContent, "B0");
  }
  assert.match(
    read("js/pages/host.js"),
    /key === "Escape"[\s\S]{0,120}gFilter\.reset\(\)/,
    "Esc on Host resets the container filter",
  );
});

test("redesign-kit-11: the toolbar's active filters are filterChips, each a Live view control, and Clear all one too", () => {
  const tb = ui.toolbar({ search: { placeholder: "Search", onInput() {} } });
  let cleared = 0;
  let all = 0;
  tb.setActive(
    [
      {
        label: "“q”",
        clear: () => cleared++,
        drive: { id: "kit-test-chip", row: "q" },
      },
    ],
    { run: () => all++, drive: { id: "kit-test-clear" } },
  );
  const chip = /** @type {any} */ (tb.el.querySelector(".nx-filterchip"));
  assert.ok(chip, "the active filter is not a filterChip");
  const x = chip.querySelector("button");
  assert.equal(x.dataset.drive, "kit-test-chip");
  x.click();
  const clear = /** @type {any} */ (
    tb.el.querySelector('[data-drive="kit-test-clear"]')
  );
  clear.click();
  assert.deepEqual([cleared, all], [1, 1]);
  assert.doesNotMatch(read("css/app.css"), /\.nx-tb__chip\b/);
});

test("redesign-kit-12: the duplicates are gone: Schedules' icon button, Host's own swatch, Secrets' hover-card restyle", () => {
  for (const f of ["js/pages/schedules.js", "css/pages/schedules.css"])
    assert.doesNotMatch(read(f), /sch-icon-btn/, f);
  assert.doesNotMatch(read("js/pages/host.js"), /const swatch\s*=/);
  for (const f of [
    "js/pages/secrets.js",
    "css/pages/secrets.css",
    "css/app.css",
  ])
    assert.doesNotMatch(read(f), /sx-tip/, f);
});

test("redesign-kit-13, kpa-10: a row menu is kp-themes' menu button on the button already in the row, not a dialog of its own", () => {
  const src = read("js/ui.js");
  const rowMenu = src.slice(
    src.indexOf("export function rowMenu"),
    src.indexOf("The entries a row menu's wrapper runs"),
  );
  assert.match(rowMenu, /kp-menu-button/);
  assert.match(rowMenu, /attachMenuButtons\(/);
  assert.match(rowMenu, /setMenu\(/);
  assert.match(rowMenu, /openMenu\(/);
  assert.doesNotMatch(
    rowMenu,
    /createElement\("dialog"\)|"dialog"/,
    "no dialog",
  );
  assert.doesNotMatch(
    src,
    /menuPlace|menuKey|pointerdown/,
    "no copy of kp's placement, keys or outside click",
  );
  assert.doesNotMatch(
    read("css/app.css"),
    /nx-rowmenu button|dialog\.nx-rowmenu/,
  );
});

// redesign-kit-13 moved to kp-themes with the menu itself: its menu button
// (js/menu-button.js) owns the listeners, the keys and Esc's focus return,
// and tests them there.

test("redesign-kit-14: the kit switches to the phone layout at one breakpoint, 48rem", () => {
  assert.equal(ui.PHONE, "(max-width: 48rem)");
  const app = read("css/app.css");
  const kit = app.slice(
    app.indexOf("BEGIN redesign 3.71.0 foundation"),
    app.indexOf("END redesign 3.71.0 foundation"),
  );
  /** @type {string[]} */
  const bad = [];
  for (const [f, css] of [
    ["app.css (foundation)", kit],
    [
      "app.css (Backups)",
      app.slice(app.indexOf("END redesign 3.71.0 foundation")),
    ],
    ["host.css", read("css/pages/host.css")],
    ["schedules.css", read("css/pages/schedules.css")],
    ["secrets.css", read("css/pages/secrets.css")],
  ])
    for (const m of css.matchAll(
      /@media[^{]*max-width:\s*([\d.]+)(rem|px)\)/g,
    )) {
      const px = Number(m[1]) * (m[2] === "rem" ? 16 : 1);
      if (px < 900 && !(m[1] === "48" && m[2] === "rem"))
        bad.push(`${f}: max-width ${m[1]}${m[2]}`);
    }
  assert.deepEqual(bad, [], "a phone rule at another width");
});

test("redesign-kit-15: the kit's spacing uses kp-themes' space tokens, not raw pixels", () => {
  const app = read("css/app.css");
  const kit = app.slice(
    app.indexOf("redesign-kit-1: the blocks"),
    app.indexOf("END redesign 3.71.0 foundation"),
  );
  const raw = kit
    .split("\n")
    .filter((l) =>
      /^\s*((row-|column-)?gap|padding[a-z-]*|margin[a-z-]*|inset[a-z-]*):/.test(
        l,
      ),
    )
    .filter((l) =>
      [...l.matchAll(/(?<![-\d.])(\d+)px/g)].some((m) => Number(m[1]) > 3),
    );
  assert.deepEqual(
    raw,
    [],
    "raw px spacing (a hairline of 1-3 px is not spacing)",
  );
  assert.match(
    read("js/ui.js"),
    /The one place outside the stack mark where a page/,
    "hueColour says why it is the exception to tokens only",
  );
});

// fail-first: the KPI toggle already refused a missing control before the
// review; finding 13 asked for this test to pin it, not for a fix.
test("redesign-kit-16: a KPI toggle without its Live view control is refused", () => {
  assert.throws(
    () =>
      ui.kpi({
        label: "Restore drills",
        toggle: { pressed: false, onToggle() {} },
      }),
    /Live view control/,
  );
  const k = ui.kpi({
    label: "Restore drills",
    toggle: { pressed: true, onToggle() {}, drive: { id: "kit-test-kpi" } },
  });
  assert.equal(k.el.dataset.drive, "kit-test-kpi");
  assert.equal(k.el.getAttribute("aria-pressed"), "true");
  assert.match(k.el.className, /\bkp-kpi\b.*\bkp-kpi--toggle\b/);
});

test("redesign-kit-16: a section's foot is hidden while it has nothing to say", () => {
  assert.equal(
    ui.section({ title: "A", desc: "a", foot: [] }).foot.hidden,
    true,
  );
  assert.equal(ui.section({ title: "A", desc: "a" }).foot.hidden, true);
  const s = ui.section({
    title: "A",
    desc: "a",
    foot: ["read with pct list", "4 s ago"],
  });
  assert.equal(s.foot.hidden, false);
  assert.equal(s.foot.children.length, 2);
});

// fail-first: the meta header's order was already right before the review;
// finding 13 asked for this test to pin it, not for a fix.
test("redesign-kit-16, redesign-final X1: a meta header reads the title with its live status, the description, the meta row, then the actions", () => {
  const chip = doc.createElement("span");
  const act = doc.createElement("button");
  const hd = ui.pageHeader({
    title: "Host",
    desc: "The machine.",
    meta: [chip],
    live: "measured",
    actions: [act],
  });
  // kp-themes 9.2.0's page header: the title row, the description and
  // the meta row on the left, the actions beside them.
  const inner = /** @type {any} */ (hd.el).children[0];
  assert.equal(inner.className, "kp-page-header__inner");
  const [lead, acts] = inner.children;
  assert.deepEqual(
    lead.children.map((/** @type {any} */ c) => c.className),
    [
      "kp-page-header__heading",
      "kp-page-header__description section-head__desc nx-head-desc",
      "nx-head-meta",
    ],
  );
  assert.equal(acts.className, "kp-page-header__actions nx-head-actions");
  assert.equal(hd.el.className, "kp-page-header nx-head nx-head--meta");
  // X1: the freshness has one slot on every page, right after the title.
  assert.equal(hd.title.nextElementSibling, /** @type {any} */ (hd.live).el);
  assert.ok(!hd.meta?.contains(/** @type {any} */ (hd.live).el));
});

test("redesign-kit-16: a stack the fleet does not list is neutral, never the first chart colour", () => {
  assert.equal(ui.chartColour("gone", ["admin", "notes"]), ui.NEUTRAL_COLOUR);
  assert.notEqual(ui.NEUTRAL_COLOUR, "var(--chart-1)");
});

test("redesign-kit-19: the page sheets loaded on every page keep every rule behind their page's class", () => {
  /** @type {string[]} */
  const bad = [];
  for (const [f, prefix] of [
    ["css/pages/host.css", /\.(hk-|nx-ops)/],
    ["css/pages/schedules.css", /\.sch-/],
    ["css/pages/secrets.css", /\.sx\b|\.sx-/],
  ]) {
    const css = read(/** @type {string} */ (f))
      .replace(/\/\*[\s\S]*?\*\//g, "")
      .replace(/@keyframes[^{]+\{(?:[^{}]*\{[^}]*\})*\s*\}/g, "");
    for (const m of css.matchAll(/([^{};]+)\{/g)) {
      const sel = m[1].trim();
      if (!sel || sel.startsWith("@")) continue;
      for (const one of sel.split(","))
        if (!(/** @type {RegExp} */ (prefix).test(one)))
          bad.push(`${f}: ${one.trim()}`);
    }
  }
  assert.deepEqual(bad, []);
  assert.match(
    read("index.html"),
    /load with the app, not when a\s+page mounts, on purpose/,
  );
  assert.doesNotMatch(read("js/ui.js"), /Activity, Metrics, Notifications/);
});

// redesign-openpoints-3: the stack hub kept its own grouped More menu; the
// shared more menu draws groups under headings, buttons beside links, a
// disabled entry with its reason, a danger entry, a page's own button and
// marks, and redraws only when what it shows changed (never while open).
test("redesign-openpoints-3: the shared more menu hands kp its grouped entries, runs the picked one, and redraws only on change", () => {
  /** @type {string[]} */
  const ran = [];
  /** @type {any[]} */
  const marked = [];
  const groups = (/** @type {string} */ hint) => [
    {
      group: "Data",
      items: [
        {
          label: "Restore…",
          hint,
          onClick: () => ran.push("restore"),
          attrs: { "data-action": "restore" },
          mark: (/** @type {any} */ e) => marked.push(e),
        },
        {
          label: "Change a secret…",
          hint: "rotate one",
          disabled: "Never on the dashboard's own stack",
          onClick: () => ran.push("secret"),
        },
      ],
    },
    {
      group: "Remove",
      items: [
        {
          label: "Remove…",
          hint: "gone",
          danger: true,
          onClick: () => ran.push("remove"),
        },
        { label: "Export", hint: "the files", href: "/x", download: "x.tgz" },
      ],
    },
  ];
  const more = ui.moreMenu({
    label: "Every other action on this stack (.)",
    button: {
      text: "More ▾",
      class: "kp-button",
      keys: ".",
      mark: (/** @type {any} */ b) => b.setAttribute("data-test", "more"),
    },
    groups: groups("from a snapshot"),
  });
  doc.body.append(more.el);
  const btn = /** @type {any} */ (more.button);
  assert.equal(btn.getAttribute("data-test"), "more", "the page marks it");
  assert.equal(btn.getAttribute("aria-keyshortcuts"), ".");
  assert.equal(btn.textContent, "More ▾");
  // kp-themes' menu button draws the entries and owns the keys, the
  // placing and the open and close (its own tests); this checks what the
  // dashboard hands it and that a pick runs the entry's own action.
  const list = /** @type {any} */ (more.el.querySelector('[role="menu"]'));
  assert.deepEqual(
    list
      .querySelectorAll(".kp-menu__heading")
      .map((/** @type {any} */ g) => g.textContent),
    ["Data", "Remove"],
  );
  const items = list.querySelectorAll('[role="menuitem"]');
  assert.equal(items.length, 4);
  assert.equal(items[0].getAttribute("data-action"), "restore");
  assert.equal(marked.length, 1);
  assert.equal(items[1].getAttribute("aria-disabled"), "true");
  assert.match(items[1].textContent, /Never on the dashboard's own stack/);
  assert.match(items[2].getAttribute("class"), /kp-menu__item--destructive/);
  assert.equal(items[3].localName, "a");
  assert.equal(items[3].getAttribute("download"), "x.tgz");
  items[2].click();
  assert.deepEqual(ran, ["remove"]);
  items[1].click();
  assert.deepEqual(ran, ["remove"], "a disabled entry does nothing");
  // A push that changes nothing keeps the drawn entries.
  more.fill(groups("from a snapshot"));
  assert.equal(list.querySelectorAll('[role="menuitem"]')[0], items[0]);
  more.stop();
  more.el.remove();
});

test("redesign-openpoints-3: the stack hub has no menu of its own", () => {
  assert.doesNotMatch(
    read("js/pages/stack.js"),
    /function groupedMenu|sh-menu/,
  );
  assert.doesNotMatch(read("css/pages/stack.css"), /sh-menu/);
});

test("kpa-1: a KPI tile is kp-themes' key figure: its parts, its tone, its meter or spark, its toggle event", () => {
  const strip = ui.kpiStrip(
    [
      {
        key: "a",
        label: "Stacks",
        value: "12",
        unit: "of 14",
        ctx: "2 parked",
        tone: "warn",
      },
      { key: "b", label: "Disk", value: "71", meter: { pct: 71, mark: 80 } },
      {
        key: "c",
        label: "CPU",
        value: "9",
        spark: [1, 2, 3, 2],
        colour: "var(--chart-2)",
      },
      { key: "d", label: "Down", value: "0", tone: "bad", href: "/x" },
    ],
    { label: "At a glance" },
  );
  assert.equal(strip.el.className, "kp-kpis");
  assert.equal(strip.el.getAttribute("data-kp-kpis-columns"), "all 3 2 1");
  assert.equal(strip.el.getAttribute("aria-label"), "At a glance");
  assert.equal(
    strip.el.style.getPropertyValue("--kpi-n"),
    "",
    "no column count of ours",
  );
  const [a, b, c, dd] = [...strip.tiles.values()].map((t) => t.el);
  assert.equal(a.className, "kp-kpi");
  assert.equal(a.querySelector(".kp-kpi__label")?.textContent, "Stacks");
  assert.equal(a.querySelector(".kp-kpi__value")?.textContent, "12of 14");
  assert.equal(a.querySelector(".kp-kpi__value small")?.textContent, "of 14");
  assert.equal(a.querySelector(".kp-kpi__trend")?.textContent, "2 parked");
  assert.equal(a.dataset.kpTone, "warning", "warn is kp's warning");
  assert.equal(dd.dataset.kpTone, "destructive", "bad is kp's destructive");
  assert.equal(dd.localName, "a");
  assert.equal(dd.getAttribute("href"), "/x");
  assert.equal(a.dataset.key, "a");
  const meter = /** @type {any} */ (b.querySelector(".kp-kpi__meter"));
  assert.ok(meter, "a meter fills the fourth row");
  assert.equal(meter.style.getPropertyValue("--kp-value"), "0.71");
  assert.equal(meter.style.getPropertyValue("--kp-mark"), "0.8");
  const spark = /** @type {any} */ (c.querySelector("svg.kp-kpi__spark"));
  assert.ok(spark, "a spark fills the fourth row");
  assert.equal(spark.getAttribute("data-kp-spark"), "1 2 3 2");
  assert.ok(spark.querySelector(".kp-kpi__spark-line"));
  assert.equal(spark.style.color, "var(--chart-2)");
  assert.equal(
    a.querySelector(".kp-kpi__meter, .kp-kpi__spark"),
    null,
    "no fourth row asked",
  );
  // A later set brings the row, then a meter replaces the spark.
  strip.tiles.get("a")?.set({ spark: [1, 2] });
  assert.ok(a.querySelector("svg.kp-kpi__spark"));
  strip.tiles.get("c")?.set({ meter: { pct: 5 } });
  assert.ok(c.querySelector(".kp-kpi__meter") && !c.querySelector("svg"));
  // Loading is its own skeleton at the final geometry.
  strip.tiles.get("a")?.set({ loading: true });
  assert.equal(a.hasAttribute("data-loading"), true);
  assert.ok(a.querySelector(".kp-skeleton"));
  strip.tiles.get("a")?.set({ value: "13" });
  assert.equal(a.hasAttribute("data-loading"), false, "a value ends loading");
});

test("kpa-1: a toggle tile has kp's hint and flips on kp's event, the page keeps the filter", () => {
  /** @type {boolean[]} */
  const calls = [];
  let pressedNow = false;
  const k = ui.kpi({
    label: "Failed",
    value: "3",
    toggle: {
      pressed: false,
      onToggle: () => calls.push((pressedNow = !pressedNow)),
      drive: { id: "kit-test-kpi" },
    },
  });
  assert.equal(k.el.localName, "button");
  assert.equal(k.el.getAttribute("aria-pressed"), "false");
  const hint = k.el.querySelector(".kp-kpi__hint");
  assert.ok(hint);
  assert.equal(
    hint.querySelector('[data-kp-when="off"]')?.textContent,
    "filter",
  );
  assert.equal(
    hint.querySelector('[data-kp-when="on"]')?.textContent,
    "filtering ×",
  );
  k.el.dispatchEvent(
    new CustomEvent("kp-kpi-toggle", {
      bubbles: true,
      detail: { pressed: true },
    }),
  );
  assert.deepEqual(calls, [true]);
  k.set({ toggle: { pressed: true, onToggle() {} } });
  assert.equal(
    k.el.getAttribute("aria-pressed"),
    "true",
    "set writes the page's state back",
  );
});

test("kpa-1: a tile's context can lead with a dot, or with a small inline meter", () => {
  const k = ui.kpi({
    label: "Backups",
    value: "9",
    ctxParts: [
      { text: "7 ok", tone: "ok" },
      { text: "2 late", tone: "warn" },
    ],
  });
  const dots = k.el.querySelectorAll(".kp-kpi__trend .kp-health");
  assert.deepEqual(
    [...dots].map((x) => /** @type {any} */ (x).dataset.state),
    ["ok", "warn"],
  );
  const m = ui.kpi({
    label: "Disk",
    value: "7",
    ctx: "of 10",
    ctxMeter: { pct: 70 },
  });
  assert.ok(m.el.querySelector(".kp-kpi__trend > .kp-meter.kp-meter--inline"));
});

test("kpa-2, kpa-4, kpa-8: stat tiles, state badges and state words are kp-themes' own markup", () => {
  const grid = dom.statGrid([{ label: "Done", value: "3" }]);
  assert.equal(grid.className, "kp-kpis");
  assert.equal(
    grid.querySelector(".kp-kpi > .kp-kpi__label")?.textContent,
    "Done",
  );
  const t = dom.statTile("Total", "5");
  t.value.textContent = "6";
  assert.equal(t.el.querySelector(".kp-kpi__value")?.textContent, "6");
  const td = dom.badgeCell({ label: "ok", tone: "ok" });
  assert.equal(
    td.querySelector(".kp-badge")?.className,
    "kp-badge kp-badge--success",
  );
  assert.equal(
    dom.stateBadge({ label: "x", tone: "bad" }).className,
    "kp-badge kp-badge--destructive",
  );
  assert.equal(
    dom.stateBadge({ label: "x", tone: "weird" }).className,
    "kp-badge",
  );
  const w = dom.stateWord("on", ["muted"]);
  assert.equal(w.className, "kp-state-word");
  assert.equal(w.getAttribute("data-kp-words"), "on\nmuted");
  assert.equal(w.textContent, "on", "the text a table reads stays the text");
});

test("kpa-7: the live status is kp-themes' status dot beside kp's ticking line", () => {
  const l = ui.liveStatus("read");
  assert.match(l.el.className, /\bkp-health\b.*\bnx-live\b/);
  assert.ok(l.el.querySelector(".kp-health__dot"));
  assert.ok(l.el.querySelector("time[data-kp-ago]"), "kp's freshness line");
  assert.equal(l.el.dataset.state, undefined, "not read yet: the muted dot");
  l.set(1_700_000_000);
  assert.equal(l.el.dataset.state, "ok");
  l.set(null, "reading 2 of 6…");
  assert.equal(l.el.dataset.state, undefined);
  assert.equal(l.el.hasAttribute("aria-busy"), true);
  l.fail("not read");
  assert.equal(l.el.dataset.state, "down");
  assert.equal(l.el.hasAttribute("aria-busy"), false);
});

test("kpa-19: the stepper is kp's wizard steps, the note a kp info alert that remembers its dismissal", () => {
  const st = ui.stepper(["Pick", "Check", "Run"]);
  assert.equal(st.el.className, "kp-wizard__steps");
  st.set(2);
  const li = st.el.querySelectorAll("li");
  assert.deepEqual(
    [...li].map((x) => /** @type {any} */ (x).dataset.state),
    ["done", "current", undefined],
  );
  assert.equal(li[1].getAttribute("aria-current"), "step");
  assert.equal(li[0].hasAttribute("aria-current"), false);
  const rows = ui.checkList([
    { title: "A", desc: "a", state: "run" },
    { title: "B", desc: "b", state: "ok" },
  ]);
  assert.ok(
    rows.querySelector('li[data-s="run"] .nx-check__st > .kp-spinner'),
    "the running step spins with kp's spinner",
  );
  assert.equal(rows.querySelector('li[data-s="ok"] .kp-spinner'), null);
  localStorage.removeItem("kit-note");
  const n = ui.explainNote("kit-note", ["Words"]);
  assert.ok(n);
  assert.match(n.el.className, /\bkp-alert\b.*\bkp-alert--info\b/);
  assert.match(n.close.className, /\bkp-alert__close\b/);
  n.el.dispatchEvent(new CustomEvent("kp-alert-dismiss", { bubbles: true }));
  assert.equal(localStorage.getItem("kit-note"), "1");
  assert.equal(ui.explainNote("kit-note", ["Words"]), null, "gone for good");
});
