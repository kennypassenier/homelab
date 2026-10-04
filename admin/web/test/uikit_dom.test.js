// redesign-kit-4..19 (the senior review of the shared kit, 2026-10-03):
// the shared blocks of ui.js drawn into a small DOM (support/minidom.mjs)
// and read back — the Live view controls they carry, the states they
// switch between, the keyboard — plus the guards over the stylesheets and
// sources the review found duplicated. Each `redesign-kit-N` names its
// REGISTER row.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { install, press, pointerDown, shiftClick } from "./support/minidom.mjs";

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
    (b) => `${b.dataset.v}:${b.getAttribute("aria-pressed")}`,
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
  assert.equal(region.querySelector(".kp-toast"), null, "Undo closed it");
  ui.toast("plain one", { ms: 0 });
  const last = ui.toast("plain two", { ms: 0 });
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
  assert.match(js, /section\(\{/, "Secrets does not use section");
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
    ".sx .nx-kbd",
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
  /** @type {any} */ (sw.el.querySelector('[data-v="x"]')).click();
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

  /** @type {unknown[]} */
  const got = [];
  const th = ui.sortHead({
    label: "Size",
    key: "size",
    sort: [{ key: "size", dir: 1 }],
    onSort: (next) => got.push(next),
    remember: "kit-head",
    drive: { id: "kit-test-sort", row: "size" },
  });
  assert.equal(th.getAttribute("aria-sort"), "ascending");
  const btn = /** @type {any} */ (th.querySelector("button"));
  assert.equal(btn.dataset.drive, "kit-test-sort");
  btn.click();
  assert.deepEqual(got, [[{ key: "size", dir: -1 }]]);
  assert.deepEqual(rememberedSort("kit-head"), [{ key: "size", dir: -1 }]);
});

test("redesign-kit-7: chip, dot, meter, shareBar, failBox, rowKeys, drawer and keyRow are shared blocks", () => {
  const c = ui.chip("!", { tone: "warn", label: "unreadable" });
  assert.equal(c.className, "nx-chip nx-chip--warn");
  assert.equal(c.getAttribute("aria-label"), "unreadable");
  assert.equal(ui.dot("ok", "running").className, "nx-dot nx-dot--ok");
  assert.equal(ui.dot("bad").getAttribute("aria-hidden"), "true");
  const m = ui.meter({ pct: 140, mark: 46, tone: "bad" });
  assert.equal(m.el.className, "nx-meter nx-meter--bad");
  assert.equal(
    /** @type {any} */ (m.el.querySelector("i")).style.width,
    "100%",
  );
  assert.equal(/** @type {any} */ (m.el.querySelector("b")).style.left, "46%");
  const sb = ui.shareBar(41, "41 %", "var(--chart-2)");
  assert.equal(sb.querySelector(".nx-share__num")?.textContent, "41 %");
  assert.equal(/** @type {any} */ (sb.querySelector("i")).style.width, "41%");

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

  const list = /** @type {any} */ (doc.createElement("ul"));
  for (const n of ["a", "b", "c"]) {
    const li = doc.createElement("li");
    li.setAttribute("tabindex", "0");
    li.dataset.n = n;
    list.append(li);
  }
  doc.body.append(list);
  /** @type {string[]} */
  const opened = [];
  const stop = ui.rowKeys(list, "li", (r) => opened.push(r.dataset.n ?? ""));
  const [a, , cc] = list.querySelectorAll("li");
  a.focus();
  press(a, "j");
  assert.equal(doc.activeElement.dataset.n, "b");
  press(doc.activeElement, "End");
  assert.equal(doc.activeElement, cc);
  press(cc, "k");
  press(doc.activeElement, "Enter");
  assert.deepEqual(opened, ["b"]);
  stop();
  press(doc.activeElement, "j");
  assert.equal(doc.activeElement.dataset.n, "b", "stop() stops listening");
  list.remove();

  let closed = 0;
  const d = ui.drawer({
    title: "New schedule",
    desc: "Read it as a sentence.",
    body: "body",
    onClose: () => closed++,
  });
  d.open();
  assert.ok(d.el.open && d.el.isConnected);
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

test("redesign-kit-8: a sortable plain table's headers are Live view controls and sort on several keys, remembered", () => {
  assert.throws(
    () =>
      ui.sortableTable(
        /** @type {any} */ (doc.createElement("table")),
        /** @type {any} */ ({}),
      ),
    /Live view/,
  );
  const t = /** @type {any} */ (
    dom.h(
      "table",
      null,
      dom.h(
        "thead",
        null,
        dom.h("tr", null, dom.h("th", null, "Name"), dom.h("th", null, "Size")),
      ),
      dom.h(
        "tbody",
        null,
        ...[
          ["b", "2"],
          ["a", "2"],
          ["c", "1"],
        ].map(([n, s]) =>
          dom.h("tr", null, dom.h("td", null, n), dom.h("td", null, s)),
        ),
      ),
    )
  );
  ui.sortableTable(t, {
    drive: { id: "kit-test-table" },
    remember: "kit-table",
  });
  const [name, size] = t.tHead.rows[0].cells;
  assert.deepEqual(
    [name.dataset.drive, name.dataset.driveRow],
    ["kit-test-table", "name"],
  );
  assert.equal(size.dataset.driveRow, "size");
  size.click();
  shiftClick(name);
  const order = () =>
    t.tBodies[0].children.map((/** @type {any} */ r) => r.textContent);
  assert.deepEqual(order(), ["c1", "a2", "b2"]);
  assert.equal(size.dataset.mark, "↑1");
  assert.equal(name.dataset.mark, "↑2");
  assert.deepEqual(sortstate.rememberedSort("kit-table"), [
    { key: "size", dir: 1 },
    { key: "name", dir: 1 },
  ]);
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
      /** @type {any} */ (g.el.querySelector(`[data-v="${v}"]`));
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

test("redesign-kit-13: a row menu says it is open, closes on a second click, and its keys walk the items", () => {
  const anchor = /** @type {any} */ (doc.createElement("button"));
  doc.body.append(anchor);
  const items = ["edit", "run-now", "delete"].map((name) => ({
    name,
    label: name,
    hint: name,
    run() {},
  }));
  const m = ui.rowMenu({ anchor, label: "menu", title: "menu", items });
  assert.ok(m);
  assert.equal(anchor.getAttribute("aria-expanded"), "true");
  const btns = m.el.querySelectorAll('[role="menuitem"]');
  assert.equal(doc.activeElement, btns[0]);
  press(btns[0], "ArrowDown");
  assert.equal(doc.activeElement, btns[1]);
  press(btns[1], "End");
  assert.equal(doc.activeElement, btns[2]);
  press(btns[2], "ArrowDown");
  assert.equal(doc.activeElement, btns[0], "wraps");
  press(btns[0], "Home");
  assert.equal(doc.activeElement, btns[0]);
  pointerDown(anchor);
  assert.equal(
    m.el.open,
    true,
    "pressing its own button is not a click outside",
  );
  assert.equal(
    ui.rowMenu({ anchor, label: "menu", title: "menu", items }),
    null,
  );
  assert.equal(m.el.isConnected, false, "the second click closed it");
  assert.equal(anchor.getAttribute("aria-expanded"), "false");
  anchor.remove();
  assert.match(
    read("css/app.css"),
    /dialog\.nx-rowmenu \{[^}]*box-sizing: border-box/,
    "the menu's 260 px include its padding and border",
  );
});

test("redesign-kit-13: the more menu listens on the document only while open, and Esc gives the focus back", () => {
  const before = doc.listeners("keydown");
  const more = ui.moreMenu({
    label: "More",
    items: [{ label: "Runbook", hint: "read it", href: "/runbook" }],
    drive: { id: "kit-test-more" },
  });
  doc.body.append(more.el);
  assert.equal(
    doc.listeners("keydown"),
    before,
    "a closed menu listens to nothing",
  );
  const btn = /** @type {any} */ (more.el.querySelector("button"));
  btn.click();
  assert.equal(btn.getAttribute("aria-expanded"), "true");
  assert.equal(doc.listeners("keydown"), before + 1);
  const link = /** @type {any} */ (more.el.querySelector('[role="menuitem"]'));
  link.focus();
  press(link, "Escape");
  assert.equal(btn.getAttribute("aria-expanded"), "false");
  assert.equal(
    doc.activeElement,
    btn,
    "Esc gives the focus back to the button",
  );
  assert.equal(doc.listeners("keydown"), before);
  more.el.remove();
});

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
test("redesign-kit-16: a meta header reads title, description, the meta row with the live status, then the actions", () => {
  const chip = doc.createElement("span");
  const act = doc.createElement("button");
  const hd = ui.pageHeader({
    title: "Host",
    desc: "The machine.",
    meta: [chip],
    live: "measured",
    actions: [act],
  });
  const kids = /** @type {any} */ (hd.el).children.map(
    (/** @type {any} */ c) => c.className || c.localName,
  );
  assert.deepEqual(kids, [
    "h1",
    "section-head__desc nx-head-desc",
    "nx-head-meta",
    "actions-row nx-head-actions",
  ]);
  assert.equal(hd.el.className, "nx-head nx-head--meta");
  assert.ok(
    hd.meta?.contains(/** @type {any} */ (hd.live).el),
    "the live status sits in the meta row",
  );
  assert.equal(hd.title.nextElementSibling, hd.desc);
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
test("redesign-openpoints-3: the shared more menu draws grouped entries, closes after one, on a click outside and on Esc (focus back), and redraws only on change", () => {
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
  const list = /** @type {any} */ (more.el.querySelector('[role="menu"]'));
  assert.deepEqual(
    list
      .querySelectorAll(".nx-menu__group")
      .map((/** @type {any} */ g) => g.textContent),
    ["Data", "Remove"],
  );
  const items = list.querySelectorAll('[role="menuitem"]');
  assert.equal(items.length, 4);
  assert.equal(items[0].getAttribute("data-action"), "restore");
  assert.equal(marked.length, 1);
  assert.equal(items[1].getAttribute("disabled"), "");
  assert.match(items[1].textContent, /Never on the dashboard's own stack/);
  assert.match(items[2].getAttribute("class"), /nx-menu__item--danger/);
  assert.equal(items[3].localName, "a");
  assert.equal(items[3].getAttribute("download"), "x.tgz");

  more.toggle();
  assert.equal(btn.getAttribute("aria-expanded"), "true");
  assert.equal(doc.activeElement, items[0], "the first enabled entry");
  press(items[0], "ArrowDown");
  assert.equal(doc.activeElement, items[2], "↓ skips the disabled entry");
  items[2].click();
  assert.deepEqual(ran, ["remove"]);
  assert.equal(btn.getAttribute("aria-expanded"), "false", "closed after it");

  btn.click();
  doc.body.click();
  assert.equal(btn.getAttribute("aria-expanded"), "false", "outside click");

  btn.click();
  // A push that changes nothing keeps the drawn entries (and the focus).
  more.fill(groups("from a snapshot"));
  assert.equal(list.querySelectorAll('[role="menuitem"]')[0], items[0]);
  // A change while open waits until it closes.
  more.fill(groups("from the newest snapshot"));
  assert.equal(list.querySelectorAll('[role="menuitem"]')[0], items[0]);
  items[0].focus();
  press(items[0], "Escape");
  assert.equal(btn.getAttribute("aria-expanded"), "false");
  assert.equal(doc.activeElement, btn, "Esc gives the focus back");
  assert.match(
    list.querySelectorAll('[role="menuitem"]')[0].textContent,
    /from the newest snapshot/,
  );
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

// review 4 (moved with the menu, redesign-openpoints-3): a fleet push that
// changes nothing in the stack hub's More menu does not redraw it.
test("review 4: the More menu's signature changes only when its groups do", async () => {
  const { moreGroups } = await import("../js/stackhub.js");
  const a = moreGroups({ stack: "gateway", native: false, enabled: true });
  const b = moreGroups({ stack: "gateway", native: false, enabled: true });
  assert.equal(ui.menuSignature(a), ui.menuSignature(b));
  const parked = moreGroups({
    stack: "gateway",
    native: false,
    enabled: false,
  });
  assert.notEqual(ui.menuSignature(a), ui.menuSignature(parked));
});
