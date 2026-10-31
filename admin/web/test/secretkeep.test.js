// Invariant 121 ("never a secret"): what Settings keeps for the tab and what
// chassis-rs 3.12.0 syncs from it (`sessionStorage` key
// `kp-unsaved:/settings:settings`, declared with `session_prefix` in
// shell/store.rs `browser_keys`) never holds a secret's value, and a staged
// secret leaves the list with its note ("a new value (hidden)").
//
// This runs kp-themes' OWN `keep()` (cut out of the unsaved.js chassis
// serves, so a change in kp is read here too) over the rows Settings really
// draws (`rowHtml`), in node, with no browser: a few milliseconds.
import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync } from "node:fs";
import { fileURLToPath } from "node:url";
import { install } from "./support/minidom.mjs";

install();
// kp's auto.js attaches the page on import unless the document is still
// loading; this one never finishes, so nothing starts.
Object.defineProperty(document, "readyState", { value: "loading" });
const { rowHtml } = await import("../js/settingsform.js");
const { shownValue } = await import("../js/settingsrows.js");

const SECRET = "hunter2-NEVER-STORE-ME";
const unsavedSource = readFileSync(
  fileURLToPath(import.meta.resolve("/static/kp/js/unsaved/config.js")),
  "utf8",
);
const settingsSource = readFileSync(
  new URL("../js/pages/settings.js", import.meta.url),
  "utf8",
);

/** @param {Record<string, unknown>} o */
const field = (o) =>
  /** @type {any} */ ({
    key: "k",
    group: "g",
    label: "K",
    help: "",
    default: "x",
    kind: { type: "text" },
    access: "browser",
    apply: "live",
    set: false,
    value: null,
    toml: null,
    ...o,
  });

const fields = [
  field({ key: "host_name", label: "Host name", value: "pve", set: true }),
  field({
    key: "dashboard_password",
    label: "Dashboard password",
    access: "dashboard_secret",
    set: true,
  }),
  field({
    key: "alerts_token",
    label: "Alerts token",
    access: "dashboard_secret",
  }),
];

/** The inputs of the drawn rows as minidom controls (the markup is a string). */
function controls() {
  const out = [];
  for (const f of fields) {
    const html = rowHtml(f, {
      path: "/etc/homelab/host.toml",
      term: "garuda",
      howOpen: false,
    });
    for (const m of html.matchAll(/<input\b([^>]*)>/g)) {
      const el = document.createElement("input");
      for (const a of m[1].matchAll(/([\w-]+)="([^"]*)"/g)) {
        el.setAttribute(a[1], a[2]);
        if (["type", "name", "value"].includes(a[1]))
          /** @type {any} */ (el)[a[1]] = a[2];
      }
      out.push(el);
    }
  }
  return out;
}

/** kp's own `keep()`, run on `list` with the bar Settings declares. */
/**
 * @param {any[]} list
 * @param {Record<string, string>} typed
 */
async function runKeep(list, typed) {
  const exclude = /el\.dataset\.kpKeepExclude \?\? '([^']+)'/.exec(
    unsavedSource,
  )?.[1];
  assert.ok(exclude, "kp's default exclusion selector was not found");
  /** @type {Map<string, string>} */
  const store = new Map();
  const storage = {
    setItem: (/** @type {string} */ k, /** @type {unknown} */ v) =>
      store.set(k, String(v)),
    removeItem: (/** @type {string} */ k) => store.delete(k),
    getItem: (/** @type {string} */ k) => store.get(k) ?? null,
  };
  const baseline = new Map(list.map((c) => [c, c.value]));
  for (const c of list)
    if (c.getAttribute("name") in typed)
      c.value = typed[c.getAttribute("name")];
  // kp's own ChangeKeeper (kp-themes 12 moved keep() out of unsaved.js into
  // this class), on the bar Settings declares.
  // @ts-ignore -- a kp source path only node resolves (kp-register.mjs)
  const { ChangeKeeper } = await import("/static/kp/js/unsaved/keeper.js");
  const keeper = new ChangeKeeper(
    /** @type {any} */ ({
      options: { storage },
      settings: {
        keepAttribute: "session",
        exclude,
        storeKey: "kp-unsaved:/settings:settings",
      },
      tracker: { baseline },
      win: {},
    }),
  );
  keeper.keepOn = true;
  keeper.keep();
  return store;
}

test("121: a secret typed into Settings is never kept for the tab, so never synced", async () => {
  const store = await runKeep(controls(), {
    host_name: "pve-2",
    dashboard_password: SECRET,
    alerts_token: `${SECRET}-2`,
  });
  assert.deepEqual([...store.keys()], ["kp-unsaved:/settings:settings"]);
  const kept = JSON.parse(store.get("kp-unsaved:/settings:settings") ?? "{}");
  assert.deepEqual(
    kept,
    { host_name: "pve-2" },
    "only the plain change is kept",
  );
  for (const [k, v] of store)
    assert.ok(
      !k.includes("SECRET") && !v.includes("NEVER-STORE-ME"),
      `${k} holds a secret`,
    );
});

test("121: with only a secret staged nothing is kept at all", async () => {
  const store = await runKeep(controls(), { dashboard_password: SECRET });
  assert.equal(store.size, 0, "a staged secret alone leaves no draft behind");
});

test("121: a secret's row is a password field with no value, and its note hides the text", () => {
  const html = rowHtml(fields[1], {
    path: "/p",
    term: "garuda",
    howOpen: false,
  });
  assert.match(
    html,
    /type="password"[^>]*value=""/,
    "the field is write-only and empty",
  );
  assert.ok(!html.includes(SECRET));
  assert.equal(shownValue(fields[1], SECRET), "a new value (hidden)");
  assert.equal(shownValue(fields[1], ""), "not changed");
  assert.ok(!shownValue(fields[1], SECRET).includes("hunter2"));
});

test("121: Settings keeps for the session, on kp's own exclusion, which no page setting overrides", () => {
  assert.match(settingsSource, /"data-kp-keep": "session"/);
  assert.match(settingsSource, /"data-kp-name": "settings"/);
  assert.ok(
    !/data-kp-keep-exclude/.test(settingsSource) &&
      !/kpKeepExclude/.test(settingsSource),
    "a page that overrides the exclusion could keep a password",
  );
  assert.match(
    unsavedSource,
    /kpKeepExclude \?\? '\[type="password"\], \[data-kp-secret\]'/,
    "kp's default leaves password fields and data-kp-secret out",
  );
});
