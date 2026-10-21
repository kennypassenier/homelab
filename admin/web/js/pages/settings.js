// The Settings page (redesign-settings, release 3.71.0; rebuilt on the
// approved kp-themes 11 demo `app-homelab-settings`, feat-kp11demo-5, Kenny
// 2026-10-19). How this homelab is set up: the working copy it applies from,
// who can sign in (passkeys, machine tokens, a QR to register a phone;
// `/passkeys` lands on the Sign-in section), what only this browser decides,
// and every key of the host's host.toml.
//
// The keys are edited where they stand. Every key has the editor that fits
// it (a switch, a stepper with its unit, a 24-hour time field, tags, a field,
// the key typed to confirm; keys kept over ssh show their value and the line
// to run); a changed row is marked and the Save bar (kp-themes `unsaved`)
// counts what changed, lists it as "key, was, now", reverts one key or all
// of them, stays in view, asks before the page is left and keeps the
// changes for the session. Save writes host.toml in one go: the host checks
// the change like a start and refuses it whole if anything is wrong, or if
// host.toml changed since this page read it.
//
// What holds from before: arch-edit-txn (the working copy's push, rebase or
// drop of commits the remote lacks), feat-settings-1 (host.toml read and
// written with commands answered to this session only), arch-self (a key that
// can cut the dashboard off is shown, never changed; one that can take its
// route or the backups down needs its key typed), fix-120 (per-machine tokens,
// issued and revoked here), and every Live view hook (`host-settings`).

import { send } from "../act.js";
import { notify, refusalAlarm, refusalCallout } from "../actui.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson, h } from "../dom.js";
import { editorKind, editorId, valueFromMirror } from "../settingsrows.js";
import { hostSettingsBody, isSecret } from "../editforms.js";
import { driven, register } from "../drivehooks.js";
import { declare, drivable } from "../drivable.js";
import { MIRROR, bridge, rowHtml, wire } from "../settingsform.js";
import {
  failSummary,
  fieldShown,
  foundText,
  groupsOf,
  repoState,
  sectionTarget,
  shortRemote,
  slug,
} from "../settingsview.js";
import { listen } from "../store.js";
import { mountThisBrowser } from "../thisbrowser.js";
import { highlight, pageHeader, segSwitch } from "../ui.js";
import { setParams } from "../urlstate.js";
import { editable } from "../editforms.js";
import { mountSignin } from "../signin.js";
import { chip, dot, ensureStyle } from "../ui.js";
import { attachAll } from "/static/kp/js/auto.js";
import { attachContents, setContents } from "/static/kp/js/page-contents.js";
import { attachDisabledReasons } from "/static/kp/js/reason.js";
import { setCardState } from "/static/kp/js/card-state.js";
import { setProblems } from "/static/kp/js/problems.js";
import { attachToolbars } from "/static/kp/js/toolbar.js";
import { REFRESH_EVENT, refreshDone } from "/static/kp/js/auto-refresh.js";
import { unsaved, unsavedSaved } from "/static/kp/js/unsaved.js";

// fix-239: Live view reaches every control here: host.toml's rows through
// the host-settings form (`homelab ui open host-settings`, `row edit
// <key>`), the rest by `homelab ui click …`.
const FETCH_NOW = declare({
  id: "fetch-now",
  page: "settings",
  opens: "run",
  what: "fetch the repository's working copy now",
});
const TRY_AGAIN = declare({
  id: "settings-try-again",
  page: "settings",
  opens: "view",
  what: "read what failed again",
  shows: "after the settings could not be read",
});
const REPO_CHOICE = declare({
  id: "repo-choice",
  page: "settings",
  opens: "run",
  row: "push|rebase|drop",
  what: "resolve unpushed commits: push them, rebase and push, or drop them",
  shows: "when the working copy holds commits that were not pushed",
});
const SHOW = declare({
  id: "settings-show",
  page: "settings",
  opens: "view",
  row: "all|changed|here",
  what: "show every host setting, only those changed from the default, or only those editable here",
});
const FOLD_ALL = declare({
  id: "settings-fold-all",
  page: "settings",
  opens: "view",
  what: "open or close every host.toml group",
});
const HOW_TO = declare({
  id: "settings-how-to",
  page: "settings",
  opens: "view",
  row: "<key>",
  what: "show how a key the dashboard may not change is changed over ssh",
});
const DISCARD = declare({
  id: "settings-discard",
  page: "settings",
  opens: "run",
  what: "drop every staged host.toml change (an Undo follows)",
  shows: "while a change is staged on this page",
});
const CHECK_WRITE = declare({
  id: "settings-check-and-write",
  page: "settings",
  opens: "run",
  what: "write the staged host.toml changes (Save)",
  shows: "while a change is staged on this page",
});
const SEARCH = declare({
  id: "settings-search",
  page: "settings",
  opens: "view",
  what: "the search over every setting (name, key or what it does)",
});
const NAV = declare({
  id: "settings-nav",
  page: "settings",
  opens: "view",
  row: "wc|signin|browser|<group slug>",
  what: "jump to a section from the side list",
});
const GROUP = declare({
  id: "settings-group",
  page: "settings",
  opens: "view",
  row: "<group slug>",
  what: "open or close one host.toml group",
});
const EDIT_KEY = declare({
  id: "settings-edit",
  page: "settings",
  opens: "view",
  row: "<key>",
  what: "go to one host.toml key and put the cursor in its editor",
  // The editors stand in their rows now: staging is typing, and a key goes
  // back to what it was from the Save bar's list.
  was: ["settings-stage", "settings-use-default", "settings-cancel-edit"],
});
const STAGED = declare({
  id: "settings-staged",
  page: "settings",
  opens: "view",
  what: "the Save bar's count of staged changes; it lists them, each with its own Revert",
  was: ["settings-undo"],
});
const CLEAR = declare({
  id: "settings-clear-search",
  page: "settings",
  opens: "view",
  what: "clear the search and show every setting",
  shows: "when the search matches no setting",
});

/**
 * @typedef {import("../editforms.js").HostField} HostField
 * @typedef {{commit: string, subject: string, at: number}} CommitRef
 * @typedef {import("../settingsview.js").Show} Show
 */

/**
 * A card with data: a heading with its one sentence, the tools, and the
 * stage the package fills with loading, empty and failed (`data-kp-state`).
 * A card that reads nothing (`state: "static"`, This browser) has no state to
 * draw, so it carries none.
 * @param {{id: string, title: string, desc: string, tools?: Node[],
 *   data: Node[], state?: "loading" | "ready" | "static"}} spec
 */
function dataCard(spec) {
  const data = h("div", { class: "kp-card__data" }, ...spec.data);
  const card = h(
    "section",
    {
      class: "kp-card nx-card cf-card",
      id: spec.id,
      ...(spec.state === "static"
        ? {}
        : { "data-kp-state": spec.state ?? "loading" }),
      "aria-labelledby": `${spec.id}-h`,
    },
    h(
      "div",
      { class: "kp-card__header" },
      h("h2", { class: "kp-card__title", id: `${spec.id}-h` }, spec.title),
      spec.tools?.length
        ? h("div", { class: "kp-card__actions" }, ...spec.tools)
        : null,
    ),
    h("p", { class: "kp-lede" }, spec.desc),
    h("div", { class: "kp-card__stage" }, data),
  );
  return { card, data };
}

/**
 * @param {HTMLElement} root
 * @param {{section?: string}} [opts] the section to open at (Sign-in for
 *   `/settings?section=sign-in`)
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  ensureStyle("/css/pages/config.css");
  ensureStyle("/css/pages/settings.css");
  root.classList.add("cf-page", "st-page");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  /**
   * redesign-config-11: one alert under the header per read the page lost,
   * each with its fix, worst first (kp-themes `problems`).
   * @type {Map<string, {error: import("../doctor.js").RouteError, retry: () => void}>}
   */
  const fails = new Map();
  const band = h("div", {
    class: "kp-problems nx-failband",
    "data-kp-problems": "",
    "data-kp-problems-mode": "each",
  });
  /**
   * @param {string} what
   * @param {import("../doctor.js").RouteError | null} error
   * @param {() => void} retry
   */
  const failed = (what, error, retry) => {
    if (error) fails.set(what, { error, retry });
    else fails.delete(what);
    const tryAgain = drivable(
      h("button", { type: "button", class: "kp-button" }, "Try again"),
      TRY_AGAIN,
    );
    tryAgain.addEventListener("click", () =>
      [...fails.values()].forEach((f) => f.retry()),
    );
    const s = failSummary(
      [...fails].map(([w, f]) => ({ what: w, error: f.error })),
    );
    setProblems(
      band,
      s
        ? [
            {
              key: "settings-reads",
              severity: "critical",
              what: s.title,
              why: s.why,
              remedy: s.fix ?? undefined,
              action: tryAgain,
            },
          ]
        : [],
    );
  };
  const q0 = new URLSearchParams(location.search);
  const S = {
    q: (q0.get("q") ?? "").toLowerCase(),
    /** @type {Show} */
    show: /** @type {Show} */ (
      ["changed", "here"].includes(q0.get("show") ?? "")
        ? q0.get("show")
        : "all"
    ),
  };
  const section = opts.section ?? q0.get("section");
  const keepUrl = () =>
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${setParams(location.search, {
        q: S.q,
        show: S.show === "all" ? null : S.show,
      })}`,
    );

  /** @type {{sha256: string, path: string, fields: HostField[], unknown?: string[]} | null} */
  let page = null;
  /** @type {Set<string>} keys whose name was typed to confirm */
  const typed = new Set();
  /** The host.toml changed on the host while changes were staged. */
  let stale = false;

  // ── header: the Save bar IS the page's primary action ────────────────
  const ago = agoEl("read", null, { live: true });
  const barSlot = h("div", {
    class: "kp-unsaved st-unsaved",
    id: "settings-unsaved",
    "data-kp-unsaved": "",
    "data-kp-unsaved-for": "#st-groups",
    "data-kp-name": "settings",
    "data-kp-keep": "session",
  });
  const head = pageHeader({
    title: "Host settings",
    desc: "How this homelab is set up: the working copy it applies from, who can sign in, and every key of the host's host.toml. Edit a key where it stands; nothing is written until you save.",
    titleMeta: [h("span", { class: "cf-live" }, ago)],
    actions: [barSlot],
  });

  // ── the search ────────────────────────────────────────────────────────
  const found = h("span", { class: "cf-count" });
  const searchBox = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "search",
      class: "kp-field__input kp-toolbar__input",
      placeholder: "Find a setting by name, key or what it does",
      "aria-label": "Find a setting",
      "aria-keyshortcuts": "/",
    })
  );
  searchBox.value = S.q;
  searchBox.addEventListener("input", () => {
    S.q = searchBox.value.trim().toLowerCase();
    keepUrl();
    filter();
  });
  const tb = {
    search: searchBox,
    el: h(
      "div",
      {
        class: "kp-toolbar st-tb",
        "data-kp-toolbar": "",
        "data-kp-toolbar-key": "/",
        role: "group",
        "aria-label": "Find a setting",
      },
      h(
        "div",
        { class: "kp-toolbar__search" },
        searchBox,
        h(
          "kbd",
          { class: "kp-kbd kp-toolbar__key", "aria-hidden": "true" },
          "/",
        ),
      ),
      h("div", { class: "kp-toolbar__state" }, found),
    ),
  };
  stops.push(attachToolbars(tb.el));
  drivable(searchBox, SEARCH);

  // ── working copy ──────────────────────────────────────────────────────
  const fetchNow = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "repo-sync",
        title: "Fetch the remote now and compare",
        "data-kp-disabled-reason": "The remote is being fetched just now",
      },
      "Fetch now",
    ),
    FETCH_NOW,
  );
  const wcBody = h("div", {
    class: "kp-formgrid st-working",
    "data-kp-formgrid-label": "wide",
    "data-kp-formgrid-lines": "lines",
  });
  const wc = dataCard({
    id: "wc",
    title: "Working copy",
    desc: "The stacks repository the host applies from: where it stands against its remote, how old the read of host.toml is, and which version each side runs.",
    data: [wcBody],
  });

  // ── this browser ──────────────────────────────────────────────────────
  const browserCard = dataCard({
    id: "browser",
    title: "This browser",
    desc: "Choices that belong to this device and never reach the host: the theme, calm mode, what Live view opens in, the tour, and a way to forget it all.",
    data: [],
    state: "static",
  });
  stops.push(mountThisBrowser(browserCard.data));

  // ── sign-in (passkeys, machine tokens, a QR to register a phone) ──────
  const signinBody = h("div", { class: "si-grid" });
  const signin = dataCard({
    id: "signin",
    title: "Sign-in",
    desc: "Who can reach this dashboard and the host: your passkeys for the browser, one token per machine for the CLI and TUI.",
    data: [signinBody],
    state: "static",
  });
  stops.push(
    mountSignin(signinBody, (what, e, retry) => failed(what, e, retry)),
  );

  // ── host settings ─────────────────────────────────────────────────────
  const showSeg = segSwitch({
    label: "Show",
    items: [
      { value: "all", label: "All", hint: "Show every host setting" },
      {
        value: "changed",
        label: "Changed from default",
        hint: "Show only the settings host.toml changes (or one you changed here)",
      },
      {
        value: "here",
        label: "Editable here",
        hint: "Show only the settings this page may change",
      },
    ],
    value: S.show,
    onChange: (v) => {
      S.show = /** @type {Show} */ (v);
      keepUrl();
      filter();
    },
    mark: (b, v) => void drivable(b, SHOW, v),
  });
  const foldAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Open or close every group",
      },
      "Open all / close all",
    ),
    FOLD_ALL,
  );
  foldAll.addEventListener("click", () => {
    const ds = [...groupBox.querySelectorAll("details.st-grp")].filter(
      (d) => !(/** @type {HTMLElement} */ (d).hidden),
    );
    const open = ds.some((d) => !(/** @type {HTMLDetailsElement} */ (d).open));
    for (const d of ds) /** @type {HTMLDetailsElement} */ (d).open = open;
  });
  const groupBox = h("div", {
    class: "st-groups",
    id: "st-groups",
    "data-kp-anchors": "both",
    "data-kp-anchor-levels": "3",
    "data-kp-anchor-open": "on",
  });
  const hsCard = dataCard({
    id: "host-settings",
    title: "Host configuration",
    desc: "Every key of host.toml. Checked with the host's own start-up validation before anything is written.",
    tools: [showSeg.el, foldAll],
    data: [groupBox],
  });

  // ── side list with scroll-spy (kp-themes 11.2.0's "On this page") ──────
  const side = h("nav", {
    class: "kp-contents st-side",
    "data-kp-contents": "",
    "data-kp-contents-min": "1",
    "aria-label": "Settings sections",
  });
  /** The sections the list names: this dashboard's cards, then host.toml's groups. */
  const paintSide = () => {
    const gs = page ? groupsOf(page.fields) : [];
    setContents(side, [
      { id: "wc", label: "Working copy" },
      { id: "signin", label: "Sign-in" },
      { id: "browser", label: "This browser" },
      { id: "host-settings", label: "host.toml" },
      ...gs.map((g) => ({
        id: slug(g),
        label: g,
        level: 1,
        count: page?.fields.filter((f) => f.group === g).length ?? "",
      })),
    ]);
  };
  // A group folded shut is opened first, so the press lands on its keys.
  side.addEventListener(
    "click",
    (e) => {
      const link = /** @type {Element} */ (e.target).closest(
        "a.kp-contents__link",
      );
      const id = link?.getAttribute("href")?.slice(1);
      const d = id
        ? /** @type {HTMLDetailsElement | null} */ (
            groupBox.querySelector(`details#${CSS.escape(id)}`)
          )
        : null;
      if (d) d.open = true;
    },
    true,
  );
  // Live view reaches each entry by its section.
  const markSide = () => {
    for (const a of /** @type {NodeListOf<HTMLAnchorElement>} */ (
      side.querySelectorAll("a.kp-contents__link")
    ))
      if (!a.dataset.drive)
        drivable(a, NAV, (a.getAttribute("href") ?? "").slice(1));
  };
  const sideMarks = new MutationObserver(markSide);
  sideMarks.observe(side, { childList: true, subtree: true });
  stops.push(() => sideMarks.disconnect());

  const main = h(
    "div",
    { class: "st-main" },
    wc.card,
    signin.card,
    browserCard.card,
    hsCard.card,
  );
  root.replaceChildren(
    head.el,
    band,
    tb.el,
    h("div", { class: "st-split" }, side, main),
  );
  setProblems(band, []);
  setCardState(hsCard.card, "loading", { words: "Reading host.toml…" });
  setCardState(wc.card, "loading", { words: "Reading the working copy…" });

  stops.push(attachContents(side));
  paintSide();

  // ── the Save bar ──────────────────────────────────────────────────────
  /** @type {(() => void) | null} */
  let detachBar = null;
  /** @type {HTMLElement | null} */
  let saveBtn = null;
  const attachBar = async () => {
    const off = attachAll(head.el);
    await off.ready;
    detachBar = off;
    const bar = unsaved(barSlot);
    void bar;
    saveBtn = barSlot.querySelector(".kp-unsaved__save");
    const discardBtn = barSlot.querySelector(".kp-unsaved__discard");
    const chipBtn = barSlot.querySelector(".kp-unsaved__count");
    if (saveBtn) drivable(saveBtn, CHECK_WRITE);
    if (discardBtn) drivable(/** @type {HTMLElement} */ (discardBtn), DISCARD);
    if (chipBtn) drivable(/** @type {HTMLElement} */ (chipBtn), STAGED);
  };

  /** The field of a row. @param {Element} row */
  const fieldOfRow = (row) =>
    page?.fields.find((f) => f.key === row.getAttribute("data-key"));
  /**
   * What a row's editor holds, as text for `valueFromMirror`.
   * @param {Element} row @param {HostField} f
   */
  const textOf = (row, f) => {
    const m = /** @type {HTMLInputElement | null} */ (
      row.querySelector(`[${MIRROR}]`)
    );
    if (m) return m.value;
    const kind = editorKind(f);
    if (kind === "switch")
      return /** @type {HTMLInputElement} */ (
        row.querySelector(".kp-switch__input")
      ).checked
        ? "on"
        : "off";
    const c = /** @type {HTMLInputElement | null} */ (
      row.querySelector("input.kp-field__input, textarea, select")
    );
    return c?.value ?? "";
  };

  /** Put a row's editor to the text a mirror would hold. @param {Element} row @param {HostField} f @param {string} text */
  const setRowText = (row, f, text) => {
    const m = /** @type {HTMLInputElement | null} */ (
      row.querySelector(`[${MIRROR}]`)
    );
    if (m) {
      m.value = text;
      m.dispatchEvent(new Event("input", { bubbles: true }));
      m.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    if (editorKind(f) === "switch") {
      const box = /** @type {HTMLInputElement} */ (
        row.querySelector(".kp-switch__input")
      );
      box.checked = text === "on";
      box.dispatchEvent(new Event("change", { bubbles: true }));
      return;
    }
    const c = /** @type {HTMLInputElement | null} */ (
      row.querySelector("input.kp-field__input, textarea")
    );
    if (c) {
      c.value = text;
      c.dispatchEvent(new Event("input", { bubbles: true }));
      c.dispatchEvent(new Event("change", { bubbles: true }));
    }
  };

  /** The confirm rows show their typed-name panel while the row is changed. */
  const syncConfirm = () => {
    for (const row of groupBox.querySelectorAll(
      "[data-key][data-st-kind] [data-st-confirm]",
    )) {
      const r = /** @type {HTMLElement} */ (row.closest("[data-key]"));
      const on = r.hasAttribute("data-kp-changed");
      /** @type {HTMLElement} */ (row).hidden = !on;
      if (!on) typed.delete(r.getAttribute("data-key") ?? "");
    }
  };
  groupBox.addEventListener("kp-unsaved-change", syncConfirm);
  groupBox.addEventListener("kp-type-confirm", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    const key = /** @type {Element} */ (e.target)
      .closest("[data-key]")
      ?.getAttribute("data-key");
    if (!key) return;
    if (d.matched) typed.add(key);
    else typed.delete(key);
  });
  barSlot.addEventListener("kp-unsaved-change", syncConfirm);

  /**
   * Save: the changes become the values the host takes; a key that needs its
   * name typed waits for it; the host answers once for the whole file.
   * @param {CustomEvent} e
   */
  const onSave = async (e) => {
    const changes = /** @type {{key: string, row: HTMLElement}[]} */ (
      e.detail.changes
    );
    /** @type {Record<string, string>} */
    const refused = {};
    /** @type {Map<string, {field: HostField, value: unknown}>} */
    const staged = new Map();
    const confirmed = new Set();
    for (const c of changes) {
      const f = fieldOfRow(c.row);
      if (!f) continue;
      const text = textOf(c.row, f);
      const p = valueFromMirror(f, text);
      if (!p.ok) {
        refused[c.key] = p.why;
        continue;
      }
      if (f.access === "confirm") {
        if (!typed.has(f.key)) {
          refused[c.key] = `Type ${f.key} to confirm this change.`;
          continue;
        }
        confirmed.add(f.key);
      }
      staged.set(f.key, { field: f, value: p.value });
    }
    if (Object.keys(refused).length) {
      unsavedSaved(barSlot, { refused });
      return;
    }
    // Live view: host.toml is written on the dashboard's server, once.
    if (driven() || !page) {
      unsavedSaved(barSlot, {});
      return;
    }
    const r = await send(
      "PUT",
      "/data/host-settings",
      hostSettingsBody(page.sha256, staged, confirmed),
      "the host settings",
    );
    if (!r.ok) {
      /** @type {Record<string, string>} */
      const all = {};
      for (const c of changes) all[c.key] = r.error.why;
      unsavedSaved(barSlot, { refused: all });
      errBox.replaceChildren(refusalCallout(r.error));
      await refusalAlarm(r.error, r.status);
      return;
    }
    errBox.replaceChildren();
    unsavedSaved(barSlot, {});
    saved(r.body);
  };
  barSlot.addEventListener(
    "kp-unsaved-save",
    (e) => void onSave(/** @type {CustomEvent} */ (e)),
  );
  const errBox = h("div", { class: "st-saveerr" });
  head.el.after(errBox);

  /** host.toml was written: say so, and read it again.
   * @param {any} answer */
  const saved = (answer) => {
    const s = answer.saved ?? {};
    const later = /** @type {string[]} */ (s.restart ?? []);
    const job = answer.follow?.job;
    const restarting = typeof job === "number";
    notify(
      restarting
        ? "host.toml written; restarting homelab-host.service…"
        : `host.toml written.${later.length ? ` At the host's next start: ${later.join(", ")}.` : " In force now."}`,
      "success",
      restarting
        ? {
            label: "Open the job",
            onClick: () => {
              history.pushState(null, "", `/activity?view=running&job=${job}`);
              dispatchEvent(new PopStateEvent("popstate"));
            },
          }
        : undefined,
    );
    if (answer.follow?.refused)
      errBox.replaceChildren(
        refusalCallout(answer.follow.refused, "warning", "Not restarted"),
      );
    stale = false;
    void loadHost();
  };

  // ── drawing the groups, once; filtering hides rows ───────────────────
  /** @type {(() => void) | null} */
  let detachRows = null;
  /** @type {(() => void) | null} */
  let unbridge = null;
  /** @param {HostField} f */
  const fieldByKey = (f) => f;
  void fieldByKey;

  const drawGroups = async () => {
    if (!page) return;
    const pg = page;
    detachRows?.();
    unbridge?.();
    const gs = groupsOf(pg.fields);
    const term = "garuda";
    groupBox.innerHTML = gs
      .map((g, i) => {
        const fs = pg.fields.filter((f) => f.group === g);
        const rows = fs
          .map((f) => rowHtml(f, { path: pg.path, term, howOpen: false }))
          .join("");
        return `<details class="st-grp kp-accordion__item" id="${slug(g)}" data-group="${slug(g)}" data-kp-remember="st-group-${slug(g)}"${i < 2 ? " open" : ""}><summary class="kp-accordion__summary st-grp__sum" data-st-sum="${slug(g)}"><h3 class="st-grp__title" id="h-${slug(g)}">${escHtml(g)}</h3><span class="kp-badge st-grp__count" data-st-count="${slug(g)}">${fs.length} keys</span></summary><div class="kp-accordion__body"><div class="kp-formgrid st-keylist" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines">${rows}</div></div></details>`;
      })
      .join("");
    // Live view reaches each group's summary and each how-to.
    for (const s of groupBox.querySelectorAll("[data-st-sum]"))
      drivable(
        /** @type {HTMLElement} */ (s),
        GROUP,
        s.getAttribute("data-st-sum") ?? "",
      );
    for (const s of groupBox.querySelectorAll("summary[id^='how-']"))
      drivable(
        /** @type {HTMLElement} */ (s),
        HOW_TO,
        s.id.replace(/^how-/, ""),
      );
    for (const row of groupBox.querySelectorAll("[data-key]")) {
      const key = row.getAttribute("data-key") ?? "";
      const name = /** @type {HTMLElement} */ (
        row.querySelector("[data-st-name]")
      );
      drivable(name, EDIT_KEY, key);
      name.addEventListener("click", () => focusKey(key));
    }
    detachRows = await wire(groupBox);
    unbridge = bridge(groupBox, (k) => pg.fields.find((f) => f.key === k));
    // The package's own marks (anchors, the group memory) read the groups.
    filter();
    syncConfirm();
  };

  /** @param {string} s */
  function escHtml(s) {
    return s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
  }

  /** Show only what the search and the Show choice let through. */
  function filter() {
    if (!page) return;
    const pg = page;
    const narrowed = !!S.q || S.show !== "all";
    let total = 0;
    for (const g of groupsOf(pg.fields)) {
      const all = pg.fields.filter((f) => f.group === g);
      const d = /** @type {HTMLDetailsElement | null} */ (
        groupBox.querySelector(`details#${CSS.escape(slug(g))}`)
      );
      if (!d) continue;
      let shown = 0;
      for (const f of all) {
        const row = /** @type {HTMLElement | null} */ (
          d.querySelector(`[data-key="${CSS.escape(f.key)}"]`)
        );
        if (!row) continue;
        const ok = fieldShown(
          f,
          S.q,
          S.show,
          editable,
          (k) =>
            groupBox
              .querySelector(`[data-key="${CSS.escape(k)}"]`)
              ?.hasAttribute("data-kp-changed") ?? false,
        );
        row.hidden = !ok;
        if (ok) shown += 1;
        highlightName(row, f, ok);
      }
      total += shown;
      d.hidden = shown === 0;
      if (narrowed && shown > 0) d.open = true;
      const badge = d.querySelector("[data-st-count]");
      if (badge)
        badge.textContent =
          shown === all.length
            ? `${all.length} ${all.length === 1 ? "key" : "keys"}`
            : `${shown} of ${all.length} keys`;
    }
    found.textContent = foundText(total, pg.fields.length, narrowed);
    const empty = groupBox.querySelector(".st-nomatch");
    if (total === 0 && !empty) {
      const clear = drivable(
        h("button", { type: "button", class: "kp-button" }, "Clear the search"),
        CLEAR,
      );
      clear.addEventListener("click", () => {
        S.q = "";
        if (tb.search) tb.search.value = "";
        S.show = "all";
        showSeg.set("all");
        keepUrl();
        filter();
      });
      groupBox.append(
        h(
          "div",
          { class: "kp-empty st-nomatch" },
          h("p", { class: "kp-empty__title" }, "No setting matches"),
          h(
            "p",
            { class: "kp-empty__body" },
            "Nothing in host.toml has these words in its name, key or help.",
          ),
          clear,
        ),
      );
    } else if (total > 0 && empty) empty.remove();
  }

  /** The search's match is marked in the key's name. @param {HTMLElement} row @param {HostField} f @param {boolean} ok */
  function highlightName(row, f, ok) {
    const name = row.querySelector("[data-st-name]");
    if (!name || !ok) return;
    name.replaceChildren(
      .../** @type {(Node | string)[]} */ ([highlight(f.label, S.q) ?? ""]),
    );
  }

  /** @param {string} key */
  function focusKey(key) {
    const row = groupBox.querySelector(`[data-key="${CSS.escape(key)}"]`);
    if (!row) return;
    const d = row.closest("details");
    if (d) d.open = true;
    row.scrollIntoView({ block: "center" });
    const c = /** @type {HTMLElement | null} */ (
      row.querySelector(
        "input:not([type=hidden]):not(.kp-sr-only), textarea, select, button",
      )
    );
    c?.focus();
  }

  // ── loading ───────────────────────────────────────────────────────────
  /** @returns {Promise<boolean>} */
  const loadHost = async () => {
    // A read that arrives while changes wait must not wipe them.
    const pending = groupBox.querySelectorAll("[data-kp-changed]").length;
    if (page && pending > 0 && groupBox.children.length) {
      stale = true;
      setProblems(band, [
        ...bandProblems(),
        {
          key: "settings-stale",
          severity: "warning",
          what: "host.toml changed on the host while you were editing",
          why: "Your changes are kept. Save will be refused, because the file is not the one this page read.",
          remedy:
            "Discard the changes to read the new file, or copy them down.",
        },
      ]);
      return true;
    }
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      setCardState(hsCard.card, page ? "ready" : "error", {
        title: "host.toml could not be read",
        text: r.error.why,
      });
      failed("the host settings", r.error, () => void loadHost());
      return false;
    }
    failed("the host settings", null, () => void loadHost());
    const first = page === null;
    page = r.body.page;
    paintSide();
    await drawGroups();
    await settle();
    setCardState(hsCard.card, "ready");
    // The groups just drawn are the saved state, except for what the bar
    // gave back from this tab (kp keeps staged changes for the tab and puts
    // them back as the bar attaches): marking those clean would drop them.
    if (!groupBox.querySelector("[data-kp-changed]"))
      unsaved(barSlot)?.markClean();
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
    refreshReadLine();
    if (first) goToSection();
    return true;
  };
  const bandProblems = () => [];
  /** The bar and the groups' package parts are attached before the page is called ready. */
  const settle = async () => {
    if (!detachBar) await attachBar();
  };

  let wentTo = false;
  const goToSection = () => {
    if (wentTo || !section) return;
    const id = sectionTarget(section, page ? groupsOf(page.fields) : []);
    if (!id) return;
    wentTo = true;
    const d = /** @type {HTMLDetailsElement | null} */ (
      groupBox.querySelector(`details#${CSS.escape(id)}`)
    );
    if (d) d.open = true;
    requestAnimationFrame(() =>
      document.getElementById(id)?.scrollIntoView({ block: "start" }),
    );
  };

  // ── the working copy: read freshness, versions, remote ──────────────
  /** @type {any} */
  let repoView = null;
  /** @type {{dashboard: string, host: string} | null} */
  let versions = null;
  const readLine = h("small", { class: "st-working__note" });
  const refreshHost = h("div", {
    class: "kp-refresh",
    "data-kp-refresh": "30",
    "data-kp-remember": "settings-read",
  });
  const refreshLabel = h("span", null, "Settings read from the host");
  const refreshRow = h(
    "div",
    { class: "kp-formgrid__row" },
    h("div", { class: "kp-formgrid__label" }, refreshLabel, readLine),
    h(
      "div",
      { class: "kp-formgrid__control", "data-kp-formgrid-span": "wide" },
      refreshHost,
    ),
  );
  const versionRow = h("div", { class: "kp-formgrid__row" });
  const repoRows = h("div", { class: "st-wc__rows" });
  wcBody.append(refreshRow, versionRow, repoRows);
  const refreshReadLine = () => {
    readLine.textContent = page
      ? `${page.path}, ${page.fields.length} keys`
      : "";
  };
  const wcAttach = attachAll(wcBody);
  stops.push(() => wcAttach());
  refreshHost.addEventListener(REFRESH_EVENT, async () => {
    const [a, b] = await Promise.all([loadHost(), loadRepo(), loadVersions()]);
    refreshDone(refreshHost, a && b !== false);
  });

  const drawVersions = () => {
    if (!versions) return;
    const same = versions.dashboard === versions.host;
    versionRow.replaceChildren(
      h("div", { class: "kp-formgrid__label" }, h("span", null, "Versions")),
      h(
        "div",
        {
          class: "kp-formgrid__control st-versions",
          "data-kp-formgrid-span": "wide",
        },
        h(
          "span",
          null,
          same
            ? `Dashboard and host run the same version, ${versions.host}.`
            : `Dashboard ${versions.dashboard} and host ${versions.host}: they differ.`,
        ),
        h("span", {
          class: "kp-badge kp-badge--version",
          "data-kp-version-scheme": "numbers",
          "data-kp-version-from": versions.host,
          "data-kp-version-to": versions.dashboard,
          "data-kp-version-app": "homelab",
          "data-kp-version-href":
            "https://github.com/kennypassenier/homelab/releases",
        }),
      ),
    );
    void attachAll(versionRow);
  };
  const loadVersions = async () => {
    const r = await fetchJson("/data/versions", "the versions", abort.signal);
    if (r.ok) {
      versions = { dashboard: r.body.dashboard, host: r.body.host };
      drawVersions();
    }
    return r.ok;
  };

  /** @param {{label: string, value: Node | string, tone?: string}[]} rows */
  const repoLines = (rows) =>
    rows.map((r) =>
      h(
        "div",
        { class: "kp-formgrid__row" },
        h("div", { class: "kp-formgrid__label" }, h("span", null, r.label)),
        h(
          "div",
          {
            class: "kp-formgrid__control",
            "data-kp-formgrid-span": "wide",
          },
          r.value,
        ),
      ),
    );

  const loadRepo = async () => {
    const r = await fetchJson("/data/repo", "the working copy", abort.signal);
    if (!r.ok) {
      setCardState(wc.card, repoView ? "ready" : "error", {
        title: "The working copy could not be read",
        text: r.error.why,
      });
      failed("the working copy", r.error, () => void loadRepo());
      return false;
    }
    failed("the working copy", null, () => void loadRepo());
    drawRepo(r.body);
    setCardState(wc.card, "ready");
    return true;
  };
  wc.card.addEventListener("kp-card-retry", () => {
    setCardState(wc.card, "loading", { words: "Reading the working copy…" });
    void loadRepo();
  });
  hsCard.card.addEventListener("kp-card-retry", () => {
    setCardState(hsCard.card, "loading", { words: "Reading host.toml…" });
    void loadHost();
  });

  /**
   * @param {{repo: {present: boolean, remote: string, branch: string,
   *   head: CommitRef | null, unpushed: CommitRef[], behind: number,
   *   dirty: string[], key_present: boolean | null, fetched_at: number | null,
   *   error: string | null}, deployed_unpushed: {stack: string, commit: string}[]}} v
   */
  const drawRepo = (v) => {
    repoView = v;
    const r = v.repo;
    const st = repoState(r);
    const fetched = agoEl("Last fetched", r.fetched_at ?? null);
    /** @type {Node[]} */
    const alerts = [];
    if (r.error)
      alerts.push(
        h(
          "div",
          {
            class: "kp-alert kp-alert--warning",
            role: "status",
            "data-kp-formgrid-span": "all",
          },
          `The last fetch or clone failed: ${r.error}`,
        ),
      );
    if (r.dirty.length)
      alerts.push(
        h(
          "div",
          {
            class: "kp-alert kp-alert--destructive",
            role: "alert",
            "data-kp-formgrid-span": "all",
          },
          `Files differ from the last commit: ${r.dirty.join(", ")}. Edits are refused until that is looked at by hand.`,
        ),
      );
    if (r.unpushed.length) {
      const deployed = v.deployed_unpushed.map((d) => d.stack);
      alerts.push(
        h(
          "div",
          {
            class: "kp-alert kp-alert--warning",
            role: "status",
            "data-kp-formgrid-span": "all",
          },
          h(
            "div",
            { class: "kp-alert__body" },
            h(
              "strong",
              null,
              `${r.unpushed.length} commit(s) here are not on the remote. `,
            ),
            deployed.length
              ? `The host runs ${deployed.join(", ")} from one of them: push keeps the repository and the fleet in step.`
              : "Push them, rebase them onto the remote, or drop them.",
            h(
              "ul",
              null,
              r.unpushed.map((c) =>
                h(
                  "li",
                  { class: "cf-mono" },
                  `${c.commit.slice(0, 10)} · ${c.subject}`,
                ),
              ),
            ),
            h(
              "div",
              { class: "kp-formgrid__buttons" },
              /** @type {const} */ (["push", "rebase", "drop"]).map((choice) =>
                resolveButton(choice, async () => void (await loadRepo())),
              ),
            ),
          ),
        ),
      );
    }
    repoRows.replaceChildren(
      ...repoLines([
        {
          label: "At",
          value: h(
            "span",
            { class: "st-wc__at" },
            h(
              "strong",
              null,
              r.head ? r.head.subject : r.present ? "—" : "not cloned yet",
            ),
            r.head
              ? h(
                  "span",
                  { class: "cf-chip cf-chip--mono", title: r.head.commit },
                  r.head.commit.slice(0, 10),
                )
              : null,
            chip(r.branch),
            h(
              "span",
              { class: "cf-mono cf-muted st-remote", title: r.remote },
              shortRemote(r.remote),
            ),
          ),
        },
        {
          label: "State",
          value: h(
            "span",
            { class: "st-wc__chips" },
            dot(st.tone, st.word),
            h(
              "span",
              {
                class: "cf-chip",
                title: "Commits on the remote the host has not fetched",
              },
              `${r.behind} behind`,
            ),
            h(
              "span",
              {
                class: "cf-chip",
                title:
                  "Commits here the remote lacks: push, rebase or drop them",
              },
              `${r.unpushed.length} unpushed`,
            ),
            h(
              "span",
              { class: "cf-chip", title: "Files changed but not committed" },
              `${r.dirty.length} uncommitted`,
            ),
            r.key_present === null
              ? null
              : h(
                  "span",
                  {
                    class: `cf-chip${r.key_present ? "" : " cf-chip--warn"}`,
                    title: "The deploy key the dashboard pushes with",
                  },
                  r.key_present ? "deploy key present" : "deploy key missing",
                ),
          ),
        },
      ]),
      h(
        "div",
        { class: "kp-formgrid__row" },
        h(
          "div",
          { class: "kp-formgrid__label" },
          h("span", null, "Remote"),
          h("small", null, r.fetched_at ? fetched : "Not fetched yet"),
        ),
        h(
          "div",
          {
            class: "kp-formgrid__control st-versions",
            "data-kp-formgrid-span": "wide",
          },
          fetchNow,
          h(
            "small",
            { class: "cf-hint" },
            "Push, rebase or drop appear here when the remote lacks commits.",
          ),
        ),
      ),
      ...alerts,
    );
    // The fetch button is drawn here, after the page was wired once.
    void attachDisabledReasons(wcBody);
  };

  // redesign-config-10: the fetch says it runs (it can take seconds on a
  // slow remote), and a page left meanwhile neither toasts nor reads.
  fetchNow.addEventListener("click", async () => {
    const b = /** @type {HTMLButtonElement} */ (fetchNow);
    b.disabled = true;
    b.setAttribute("aria-busy", "true");
    b.textContent = "Fetching the remote…";
    const x = await send(
      "POST",
      "/data/repo/sync",
      undefined,
      "the working copy",
    );
    b.disabled = false;
    b.removeAttribute("aria-busy");
    b.textContent = "Fetch now";
    if (abort.signal.aborted) return;
    if (!x.ok) await refusalAlarm(x.error, x.status);
    else notify("The working copy is up to date.", "success");
    if (!abort.signal.aborted) void loadRepo();
  });

  // ── Live view: the hooks the replay works this page with ─────────────
  let lastSaved = "";
  const unregister = register("host-settings", {
    ready: () => page !== null && detachBar !== null,
    openKey: (/** @type {string} */ key) => focusKey(key),
    rowButton: (/** @type {string} */ key) => {
      const row = groupBox.querySelector(`[data-key="${CSS.escape(key)}"]`);
      const d = row?.closest("details");
      if (d) d.open = true;
      return /** @type {HTMLElement | null} */ (
        row?.querySelector("[data-st-name]") ?? null
      );
    },
    reviewButton: () =>
      saveBtn && !(/** @type {HTMLButtonElement} */ (saveBtn).disabled)
        ? saveBtn
        : null,
    setStaged: (
      /** @type {Record<string, unknown>} */ want,
      /** @type {string[]} */ typedKeys,
    ) => {
      if (!page) return;
      for (const f of page.fields) {
        const row = groupBox.querySelector(`[data-key="${CSS.escape(f.key)}"]`);
        if (!row) continue;
        if (f.key in want) {
          const v = want[f.key];
          setRowText(row, f, wantText(f, v));
        } else if (row.hasAttribute("data-kp-changed")) {
          const m = row.querySelector(`[${MIRROR}]`);
          setRowText(row, f, m?.getAttribute("value") ?? "");
        }
      }
      for (const k of typedKeys) typed.add(k);
    },
    openReview: () =>
      /** @type {HTMLElement | null} */ (
        barSlot.querySelector(".kp-unsaved__count")
      )?.click(),
    showSaved: (/** @type {any} */ answer) => {
      const key = JSON.stringify(answer);
      if (key === lastSaved) return;
      lastSaved = key;
      saved(answer);
    },
  });
  /** The text a staged value is in a row's editor. @param {HostField} f @param {unknown} v */
  const wantText = (f, v) => {
    const kind = editorKind(f);
    if (v == null) return kind === "switch" ? "off" : "";
    if (kind === "switch") return v === true ? "on" : "off";
    if (kind === "hour") return `${String(v).padStart(2, "0")}:00`;
    if (Array.isArray(v)) return v.join(",");
    return String(v);
  };

  const offRepo = listen("repo", (v) => drawRepo(v));
  const offHost = listen("host_settings", () => void loadHost());
  void loadRepo().catch(() => {});
  void loadHost().catch((e) => console.error("loadHost", e));
  void loadVersions().catch(() => {});
  return () => {
    unregister();
    abort.abort();
    offRepo();
    offHost();
    detachRows?.();
    unbridge?.();
    detachBar?.();
    for (const s of stops) s();
    void stale;
    void editorId;
    void isSecret;
    root.classList.remove("cf-page", "st-page");
  };
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
function resolveButton(choice, reload) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button kp-button--sm${choice === "drop" ? " kp-button--destructive" : ""}`,
      "data-choice": choice,
      title: {
        push: "Push the commits to the remote",
        rebase: "Put the commits on top of the remote's, then push",
        drop: "Throw the commits away; the working copy goes back to the remote",
      }[choice],
    },
    { push: "Push", rebase: "Rebase and push", drop: "Drop them…" }[choice],
  );
  drivable(b, REPO_CHOICE, choice);
  b.addEventListener("click", async () => {
    if (choice === "drop") {
      const { openConfirmation } = await import("/static/kp/js/components.js");
      openConfirmation(b, {
        phrase: "Drop the unpushed commits?",
        typeName: "drop",
        loses: [
          "The working copy goes back to the remote's branch.",
          "Those commits are gone from here.",
        ],
        acceptLabel: "Drop the commits",
        onAccept: () => void resolve(choice, reload),
      });
      return;
    }
    await resolve(choice, reload);
  });
  return b;
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
async function resolve(choice, reload) {
  const r = await send(
    "POST",
    "/data/repo/unpushed",
    choice === "drop" ? { choice, confirm: "drop" } : { choice },
    "the unpushed commits",
  );
  if (!r.ok) await refusalAlarm(r.error, r.status);
  else notify(choice === "drop" ? "Dropped." : "Pushed.", "success");
  void reload();
}
