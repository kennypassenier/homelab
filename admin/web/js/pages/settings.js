// The Settings page (redesign-settings, release 3.71.0; Kenny approved
// the demo 2026-10-03: ~/.local/share/homelab/redesign-3.71/settings.html,
// implemented exactly). How this homelab is set up: the working copy it
// applies from, who can sign in (passkeys and machine tokens, `/passkeys`
// lands on the Sign-in section), and every key of the host's host.toml.
//
// Layout: the header carries the staged-change summary as its primary
// action ("Check and write 2…", with Discard and the "2 changes staged"
// chip whose tip lists them); a search over every setting; a side list
// with scroll-spy (This dashboard: Working copy, Sign-in; host.toml: one
// entry per group with its count); then the cards. host.toml's groups
// are folded cards, each key one row: name, key and help · value (or
// "default …") · when it takes effect and who may change it · Edit (an
// inline editor: Stage / Cancel, Enter and Esc) or "How to…" (the ssh
// way, for keys the dashboard must never change).
//
// What holds from before: arch-edit-txn (the working copy's push, rebase
// or drop of commits the remote lacks), feat-settings-1 (host.toml read
// and written with commands answered to this session only; the review
// dialog checks with the host's start-up validation and restarts the host
// when a key needs it), arch-self (a key that can cut the dashboard off is
// shown, never changed; one that can take its route or the backups down
// needs its key typed), fix-120 (per-machine tokens, issued and revoked
// here), and every Live view hook (`host-settings`, `key`, `host-review`,
// `tokens-issue`).

import { send } from "../act.js";
import { notify, openDialog, refusalAlarm, refusalCallout } from "../actui.js";
import { agoEl, setAgo } from "../ago.js";
import { fetchJson } from "../dom.js";
import {
  editable,
  fieldText,
  hostSettingsBody,
  isSecret,
  parseKey,
  valueText,
} from "../editforms.js";
import { markErrors } from "../editui.js";
import { driven, register } from "../drivehooks.js";
import { declare, drivable, viaForm } from "../drivable.js";
import { mountJobPanel } from "../jobpanel.js";
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
} from "../settingsview.js";
import { listen } from "../store.js";
import { toolbar } from "../ui.js";
import { setParams } from "../urlstate.js";
import {
  chip,
  dot,
  el,
  failBox,
  hideTip,
  highlight,
  pageHeader,
  pageStyles,
  seg,
  skel,
  tipOn,
} from "./configkit.js";
import { mountPasskeys } from "./passkeys.js";

// fix-239: Live view reaches every control here: host.toml's rows through
// the host-settings form (`homelab ui open host-settings`, `row edit
// <key>`), the rest by `homelab ui click …`.
const ISSUE_TOKEN = declare({
  id: "issue-token",
  page: "settings",
  opens: "dialog",
  what: "open the Issue a new token dialog",
});
const REVOKE_TOKEN = declare({
  id: "revoke-token",
  page: "settings",
  opens: "dialog",
  row: "<token name>",
  what: "ask to revoke one machine's token",
});
const FETCH_NOW = declare({
  id: "fetch-now",
  page: "settings",
  opens: "run",
  what: "fetch the repository's working copy now",
});
const REPO_CHOICE = declare({
  id: "repo-choice",
  page: "settings",
  opens: "run",
  row: "push|rebase|drop",
  what: "resolve unpushed commits: push them, rebase and push, or drop them",
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
});
const CHECK_WRITE = declare({
  id: "settings-check-and-write",
  page: "settings",
  opens: "dialog",
  what: "open the review of the staged changes (Check and write)",
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
  row: "wc|signin|<group slug>",
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
  what: "open the inline editor of one host.toml key",
});
const UNDO_KEY = declare({
  id: "settings-undo",
  page: "settings",
  opens: "view",
  row: "<key>",
  what: "drop one staged change",
});
const STAGE = declare({
  id: "settings-stage",
  page: "settings",
  opens: "view",
  what: "stage the value typed in the open editor (nothing is written yet)",
});
const USE_DEFAULT = declare({
  id: "settings-use-default",
  page: "settings",
  opens: "view",
  what: "stage the open key's removal: the host takes its default",
});
const CANCEL_EDIT = declare({
  id: "settings-cancel-edit",
  page: "settings",
  opens: "view",
  what: "close the open editor and keep the old value",
});
const STAGED = declare({
  id: "settings-staged",
  page: "settings",
  opens: "view",
  what: "the header's staged-change chip; focus or hover lists the staged changes",
});
const CLEAR = declare({
  id: "settings-clear-search",
  page: "settings",
  opens: "view",
  what: "clear the search and show every setting",
});

/**
 * @typedef {import("../editforms.js").HostField} HostField
 * @typedef {{commit: string, subject: string, at: number}} CommitRef
 * @typedef {import("../settingsview.js").Show} Show
 */

/**
 * @param {HTMLElement} root
 * @param {{section?: string}} [opts] the section to open at (Sign-in for
 *   `/settings?section=sign-in`)
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  pageStyles("settings");
  root.classList.add("cf-page", "st-page");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  const q0 = new URLSearchParams(location.search);
  const S = {
    q: (q0.get("q") ?? "").toLowerCase(),
    /** @type {Show} */
    show: /** @type {Show} */ (
      ["changed", "here"].includes(q0.get("show") ?? "")
        ? q0.get("show")
        : "all"
    ),
    /** @type {string | null} the key being edited inline */
    editing: null,
    /** @type {string | null} the key whose ssh how-to is open */
    howto: null,
    /** @type {Set<string>} open groups (the first two start open) */
    open: new Set(),
    /** @type {string | null} the inline editor's error */
    err: null,
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
  /** @type {Map<string, {field: HostField, value: unknown}>} */
  const changes = new Map();
  /** @type {Set<string>} */
  const confirmed = new Set();

  // ── header: the staged-change summary IS the primary action ───────────
  const ago = agoEl("read", null, { live: true });
  const stagedChip = drivable(
    el("span", { class: "cf-chip", tabindex: "0" }),
    STAGED,
  );
  tipOn(stagedChip, () =>
    changes.size
      ? [
          el("b", null, "Staged, not written yet"),
          ...[...changes.values()].map(({ field, value }) =>
            el(
              "span",
              { class: "cf-tip__row" },
              el(
                "span",
                { class: "cf-mono" },
                `${field.key}: ${valueText(field)} → ${pendingText(field, value)}`,
              ),
              el(
                "span",
                { class: "cf-hint" },
                field.apply === "live" ? "at once" : "next start",
              ),
            ),
          ),
          el(
            "span",
            { class: "cf-hint" },
            "Check and write validates them together with the host's own start-up checks.",
          ),
        ]
      : null,
  );
  const discard = /** @type {HTMLButtonElement} */ (
    viaForm(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--secondary",
          title: "Drop every staged change (you can undo)",
          onclick: () => {
            const before = new Map(changes);
            const typed = new Set(confirmed);
            changes.clear();
            confirmed.clear();
            paint();
            notify(`Dropped ${before.size} staged change(s).`, "info", {
              label: "Undo",
              onClick: () => {
                for (const [k, v] of before) changes.set(k, v);
                for (const k of typed) confirmed.add(k);
                paint();
              },
            });
          },
        },
        "Discard",
      ),
      "host-settings",
    )
  );
  const write = /** @type {HTMLButtonElement} */ (
    viaForm(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          id: "settings-save",
          title:
            "Validate with the host's start-up checks, then write host.toml in one go",
          onclick: () => openReview(),
        },
        "Check and write",
      ),
      "host-settings",
    )
  );
  drivable(discard, DISCARD);
  drivable(write, CHECK_WRITE);
  const head = pageHeader({
    title: "Settings",
    desc: "How this homelab is set up: the working copy it applies from, who can sign in, and every key of the host's host.toml. Edits are staged, checked and written in one go.",
    meta: [el("span", { class: "cf-live" }, ago)],
    actions: [stagedChip, discard],
    primary: write,
  });

  // ── the search ────────────────────────────────────────────────────────
  const found = el("span", { class: "cf-count" });
  const tb = toolbar({
    search: {
      placeholder: "Find a setting by name, key or what it does",
      label: "Find a setting",
      value: S.q,
      onInput: (v) => {
        S.q = v.trim().toLowerCase();
        keepUrl();
        paintGroups();
      },
    },
    groups: [],
    state: [found],
  });
  tb.el.classList.add("st-tb");
  if (tb.search) drivable(tb.search, SEARCH);

  // ── working copy ──────────────────────────────────────────────────────
  const wcBody = el("div", { class: "st-wc" }, skel("50%"), skel("30%"));
  const wcFoot = el(
    "div",
    { class: "cf-card__foot" },
    el("span", null, skel("20ch")),
    el(
      "span",
      null,
      "Push, rebase or drop appear here when the remote lacks commits",
    ),
  );
  const fetchNow = drivable(
    el(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "repo-sync",
        title: "Fetch the remote now and compare",
      },
      "Fetch now",
    ),
    FETCH_NOW,
  );
  const wc = card({
    id: "wc",
    title: "Working copy",
    desc: "The stacks repository the host applies from: where it stands against its remote.",
    tools: [fetchNow],
    body: [wcBody],
    foot: wcFoot,
  });

  // ── sign-in ───────────────────────────────────────────────────────────
  const passkeyCol = el("div", { class: "si-col" });
  const tokenList = el("div", { class: "si-list" }, skel("60%"), skel("40%"));
  const issueBtn = drivable(
    el(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "tokens-issue",
        title: "Issue a bearer token for one machine; it is shown once",
      },
      "Issue token…",
    ),
    ISSUE_TOKEN,
  );
  const tokenCol = el(
    "div",
    { class: "si-col" },
    el(
      "div",
      { class: "si-col__head" },
      el("h3", null, "Machine tokens"),
      issueBtn,
    ),
    tokenList,
  );
  const signin = card({
    id: "signin",
    title: "Sign-in",
    desc: "Who can reach this dashboard and the host: your passkeys for the browser, one token per machine for the CLI and TUI.",
    body: [el("div", { class: "si-grid" }, passkeyCol, tokenCol)],
  });
  stops.push(mountPasskeys(passkeyCol));

  // ── host settings ─────────────────────────────────────────────────────
  const hsDesc = el(
    "p",
    { class: "section-head__desc" },
    "Every key of host.toml. Checked with the host's own start-up validation before anything is written.",
  );
  const showSeg = seg({
    label: "Show",
    options: [
      { value: "all", label: "All", hint: "Show every host setting" },
      {
        value: "changed",
        label: "Changed from default",
        hint: "Show only the settings host.toml changes (or one you staged)",
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
      paintGroups();
    },
    mark: (b, v) => void drivable(b, SHOW, v),
  });
  const foldAll = drivable(
    el(
      "button",
      {
        type: "button",
        class: "cf-linkbtn",
        title: "Open or close every group",
        onclick: () => {
          if (!page) return;
          const gs = groupsOf(page.fields);
          S.open = S.open.size === gs.length ? new Set() : new Set(gs);
          paintGroups();
        },
      },
      "Open all / close all",
    ),
    FOLD_ALL,
  );
  const hsHead = el(
    "div",
    { class: "st-hs-head", id: "host-settings" },
    el("div", null, el("h2", null, "Host settings"), hsDesc),
    el("div", { class: "cf-row" }, showSeg.el, foldAll),
  );
  const groupBox = el(
    "div",
    { class: "st-groups" },
    ...Array.from({ length: 4 }, () =>
      el(
        "div",
        { class: "kp-card nx-card st-grp st-grp--sk" },
        skel("30%"),
        skel("60%"),
      ),
    ),
  );

  // ── side list with scroll-spy ─────────────────────────────────────────
  const side = el("nav", {
    class: "st-side",
    "aria-label": "Settings sections",
  });
  /** @param {string} current */
  const paintSide = (current) => {
    const gs = page ? groupsOf(page.fields) : [];
    /** @param {string} id @param {string} label @param {string} n */
    const link = (id, label, n) =>
      drive(
        "a",
        NAV,
        id,
        {
          href: `#${id}`,
          "aria-current": String(id === current),
          onclick: (/** @type {Event} */ e) => {
            e.preventDefault();
            const g = gs.find((x) => slug(x) === id);
            if (g && !S.open.has(g)) {
              S.open.add(g);
              paintGroups();
            }
            document
              .getElementById(id)
              ?.scrollIntoView({ behavior: "smooth", block: "start" });
          },
        },
        el("span", null, label),
        el("span", { class: "st-side__n" }, n),
      );
    side.replaceChildren(
      el("h4", null, "This dashboard"),
      link("wc", "Working copy", ""),
      link("signin", "Sign-in", ""),
      el("h4", null, "host.toml"),
      ...gs.map((g) =>
        link(
          slug(g),
          g,
          String(page?.fields.filter((f) => f.group === g).length ?? ""),
        ),
      ),
    );
  };
  paintSide("wc");

  const main = el("div", { class: "st-main" }, wc, signin, hsHead, groupBox);
  root.replaceChildren(
    head.el,
    tb.el,
    el("div", { class: "st-split" }, side, main),
  );

  let spyCurrent = "wc";
  const spy = new IntersectionObserver(
    (es) => {
      const v = es
        .filter((e) => e.isIntersecting)
        .sort((a, b) => a.boundingClientRect.top - b.boundingClientRect.top)[0];
      if (v && v.target.id !== spyCurrent) {
        spyCurrent = v.target.id;
        paintSide(spyCurrent);
      }
    },
    { rootMargin: "-80px 0px -60% 0px" },
  );
  spy.observe(wc);
  spy.observe(signin);
  stops.push(() => spy.disconnect());

  // ── painting the host settings ───────────────────────────────────────
  /** @param {HostField} f @param {unknown} v */
  const pendingText = (f, v) =>
    v == null
      ? `default (${f.default})`
      : f.kind.type === "table"
        ? "new table"
        : isSecret(f)
          ? "a new value (hidden)"
          : Array.isArray(v)
            ? v.join(", ")
            : String(v);

  const paint = () => {
    const w = stagedWords(changes.size);
    stagedChip.textContent = w.chip;
    stagedChip.className = `cf-chip${changes.size ? " cf-chip--info" : ""}`;
    discard.disabled = !changes.size;
    write.disabled = !changes.size;
    write.textContent = w.write;
    paintGroups();
  };

  /** @param {HostField} f */
  const valueView = (f) => {
    const pending = changes.get(f.key);
    if (pending)
      return el(
        "span",
        { class: "cf-row" },
        el("span", { class: "cf-mono" }, pendingText(f, pending.value)),
        chip("staged", "info"),
      );
    if (isSecret(f))
      return el("span", { class: f.set ? "" : "st-def" }, valueText(f));
    if (!f.set)
      return el(
        "span",
        { class: "st-def" },
        "default ",
        el("span", { class: "cf-mono" }, highlight(f.default, S.q)),
      );
    return el("span", { class: "cf-mono" }, valueText(f));
  };

  /** @type {(() => void) | null} */
  let unregisterKey = null;
  const closeEditor = () => {
    S.editing = null;
    S.err = null;
    unregisterKey?.();
    unregisterKey = null;
    paintGroups();
  };

  /** @param {HostField} f */
  const editor = (f) => {
    const id = `key-${f.key.replace(/_/g, "-")}`;
    const pending = changes.get(f.key);
    const start = pending
      ? f.kind.type === "bool"
        ? String(pending.value === true)
        : pending.value == null
          ? ""
          : String(pending.value)
      : f.kind.type === "bool"
        ? String(f.value === true)
        : fieldText(f);
    /** @type {HTMLInputElement | HTMLSelectElement | HTMLTextAreaElement} */
    let input;
    if (f.kind.type === "bool") {
      input = /** @type {HTMLSelectElement} */ (
        el(
          "select",
          { class: "kp-field__input", id, "aria-label": f.label },
          el("option", { value: "true" }, "on"),
          el("option", { value: "false" }, "off"),
        )
      );
    } else if (f.kind.type === "table") {
      input = /** @type {HTMLTextAreaElement} */ (
        el("textarea", {
          class: "kp-field__input cf-mono",
          id,
          rows: "6",
          spellcheck: "false",
          "aria-label": `${f.key} as TOML`,
        })
      );
    } else {
      const k = f.kind;
      input = /** @type {HTMLInputElement} */ (
        el("input", {
          class: "kp-field__input",
          // fix-143: a write-only field is masked while it is typed.
          type: isSecret(f) ? "password" : k.type === "int" ? "number" : "text",
          id,
          min: k.type === "int" ? String(k.min) : null,
          max: k.type === "int" ? String(k.max) : null,
          placeholder: f.default,
          autocomplete: "off",
          spellcheck: "false",
          "aria-label": f.label,
        })
      );
    }
    input.value = start;
    const typed = /** @type {HTMLInputElement} */ (
      el("input", {
        class: "kp-field__input",
        type: "text",
        id: `${id}-confirm`,
        autocomplete: "off",
        placeholder: f.key,
        "aria-label": `Type ${f.key} to confirm`,
      })
    );
    const err = el("span", { class: "st-err", role: "alert" }, S.err ?? "");
    const stage = (/** @type {unknown} */ value) => {
      // Live view: the change is kept on the dashboard's server.
      if (driven()) return;
      if (f.access === "confirm") {
        if (typed.value.trim() !== f.key) {
          markErrors(new Map([["confirm", typed]]), {
            confirm: `Type ${f.key} exactly.`,
          });
          return;
        }
        confirmed.add(f.key);
      }
      const same =
        value == null
          ? !f.set
          : f.set && JSON.stringify(value) === JSON.stringify(f.value);
      if (same && f.kind.type !== "table" && !isSecret(f)) {
        changes.delete(f.key);
        confirmed.delete(f.key);
      } else changes.set(f.key, { field: f, value });
      S.editing = null;
      S.err = null;
      unregisterKey?.();
      unregisterKey = null;
      paint();
    };
    const save = viaForm(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--primary",
          id: "key-stage",
          title: "Stage this value (Enter); nothing is written yet",
          onclick: () => {
            const p = parseKey(f, input.value);
            if (!p.ok) {
              S.err = p.why;
              err.textContent = p.why;
              input.setAttribute("aria-invalid", "true");
              return;
            }
            stage(p.value);
          },
        },
        "Stage",
      ),
      "host-settings",
    );
    const reset = viaForm(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: `Remove the key: the host takes its default (${f.default})`,
          onclick: () => stage(null),
        },
        "Use the default",
      ),
      "host-settings",
    );
    const cancel = viaForm(
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm kp-button--ghost",
          title: "Keep the old value (Esc)",
          onclick: closeEditor,
        },
        "Cancel",
      ),
      "host-settings",
    );
    input.addEventListener("keydown", (e) => {
      const k = /** @type {KeyboardEvent} */ (e);
      if (k.key === "Enter" && f.kind.type !== "table") {
        k.preventDefault();
        save.click();
      }
      if (k.key === "Escape") {
        k.stopPropagation();
        closeEditor();
      }
    });
    drivable(save, STAGE);
    drivable(reset, USE_DEFAULT);
    drivable(cancel, CANCEL_EDIT);
    unregisterKey?.();
    unregisterKey = register("key", {
      dialog: null,
      key: f.key,
      save: () => save,
      default: () => reset,
      cancel: () => cancel,
      close: () => closeEditor(),
    });
    setTimeout(() => input.focus(), 0);
    const hint =
      f.kind.type === "table"
        ? "Only this key: [[key]] or [key] sections. Empty removes it."
        : isSecret(f)
          ? `Currently ${f.set ? "set" : "not set"}; the value itself is never shown. Empty clears it.`
          : f.kind.type === "int"
            ? `A whole number, ${f.kind.min}–${f.kind.max}. Empty = default (${f.default}).`
            : `Empty = default (${f.default}).`;
    return [
      el(
        "span",
        { class: "st-edit-row" },
        input,
        save,
        f.set || changes.has(f.key) ? reset : null,
        cancel,
      ),
      f.access === "confirm"
        ? el(
            "label",
            { class: "st-confirm" },
            el(
              "span",
              { class: "cf-hint" },
              `Type ${f.key} to confirm: it can cut the dashboard's route or stop every backup.`,
            ),
            typed,
          )
        : null,
      err,
      el("span", { class: "cf-hint st-hint" }, hint),
    ];
  };

  /** @param {HostField} f */
  const row = (f) => {
    const acc = accessWords(f);
    const staged = changes.has(f.key);
    const editing = S.editing === f.key;
    /** @type {HTMLElement[]} */
    let edit;
    if (editable(f)) {
      const editBtn = viaForm(
        el(
          "button",
          {
            type: "button",
            class: `kp-button kp-button--sm${staged ? " kp-button--ghost" : ""}`,
            "data-key": f.key,
            title: staged ? "Change the staged value" : `Change ${f.key}`,
            onclick: () => openKey(f),
          },
          "Edit",
        ),
        "host-settings",
      );
      drivable(editBtn, EDIT_KEY, f.key);
      edit = editing
        ? []
        : staged
          ? [
              editBtn,
              drive(
                "button",
                UNDO_KEY,
                f.key,
                {
                  type: "button",
                  class: "kp-button kp-button--sm kp-button--ghost",
                  title: "Drop this staged change",
                  onclick: () => {
                    changes.delete(f.key);
                    confirmed.delete(f.key);
                    paint();
                  },
                },
                "Undo",
              ),
            ]
          : [editBtn];
    } else {
      edit = [
        drivable(
          el(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--sm kp-button--ghost",
              "aria-expanded": String(S.howto === f.key),
              title: "Show the ssh command that changes this key",
              onclick: () => {
                S.howto = S.howto === f.key ? null : f.key;
                paintGroups();
              },
            },
            "How to…",
          ),
          HOW_TO,
          f.key,
        ),
      ];
    }
    const when = el(
      "span",
      {
        class: `cf-dot cf-dot--${f.apply === "live" ? "ok" : "info"}`,
        title:
          f.apply === "live"
            ? "The host picks it up without a restart"
            : "Written now, used after the host daemon restarts",
      },
      f.apply === "live" ? "applies at once" : "at the host's next start",
    );
    return el(
      "div",
      {
        class: `st-set${staged ? " st-set--staged" : ""}${editing ? " st-set--editing" : ""}`,
        id: `k-${f.key}`,
        "data-key": f.key,
        "data-set": f.set ? "1" : "0",
      },
      el(
        "div",
        { class: "st-set__name" },
        el("strong", null, highlight(f.label, S.q)),
        el("small", { class: "cf-mono" }, highlight(f.key, S.q)),
        el(
          "span",
          { class: "st-set__help", title: f.help },
          highlight(f.help, S.q),
        ),
      ),
      el("div", { class: "st-set__val" }, editing ? editor(f) : valueView(f)),
      el(
        "div",
        { class: "st-set__when" },
        when,
        acc ? el("span", { class: "st-lock", title: acc[1] }, acc[0]) : null,
      ),
      el("div", { class: "st-set__edit" }, edit),
      S.howto === f.key
        ? el(
            "div",
            { class: "st-howto" },
            el("span", null, acc ? acc[1] : ""),
            el(
              "code",
              null,
              `ssh pve, then set ${f.key} = … in ${page?.path ?? "/etc/homelab/host.toml"}`,
            ),
            el(
              "span",
              { class: "cf-hint" },
              "Edited on the host itself; the dashboard's token is never allowed to change it. Takes effect at the host's next start.",
            ),
          )
        : null,
    );
  };

  /** @type {Map<string, HTMLDetailsElement>} */
  const groupEls = new Map();
  const paintGroups = () => {
    if (!page) return;
    const pg = page;
    const gs = groupsOf(pg.fields);
    const narrowed = !!S.q || S.show !== "all";
    let total = 0;
    for (const g of groupEls.values()) spy.unobserve(g);
    groupEls.clear();
    const cards = gs.map((g) => {
      const all = pg.fields.filter((f) => f.group === g);
      const fs = all.filter((f) =>
        fieldShown(f, S.q, S.show, editable, (k) => changes.has(k)),
      );
      total += fs.length;
      if (!fs.length) return null;
      const changed = all.filter((f) => f.set || changes.has(f.key)).length;
      const open =
        narrowed ||
        S.open.has(g) ||
        fs.some((f) => f.key === S.editing || f.key === S.howto);
      const d = /** @type {HTMLDetailsElement} */ (
        el(
          "details",
          {
            class: "kp-card nx-card st-grp",
            id: slug(g),
            open: open ? true : null,
            "data-group": g,
          },
          drive(
            "summary",
            GROUP,
            slug(g),
            { title: "Click to open or close this group" },
            el("h2", null, g),
            chip(groupChip(fs.length, all.length, changed)),
            el(
              "span",
              { class: "st-grp__meta" },
              fs
                .slice(0, 4)
                .map((f) => f.label)
                .join(" · ") + (fs.length > 4 ? " …" : ""),
            ),
          ),
          el("div", { class: "st-grp__body" }, fs.map(row)),
        )
      );
      d.addEventListener("toggle", () => {
        if (narrowed) return;
        if (d.open) S.open.add(g);
        else S.open.delete(g);
      });
      groupEls.set(g, d);
      spy.observe(d);
      return d;
    });
    groupBox.replaceChildren(
      ...cards.filter((c) => c !== null),
      ...(total
        ? []
        : [
            el(
              "div",
              { class: "cf-empty" },
              el("strong", null, "No setting matches"),
              drive(
                "button",
                CLEAR,
                null,
                {
                  type: "button",
                  class: "cf-linkbtn",
                  onclick: () => {
                    S.q = "";
                    if (tb.search) tb.search.value = "";
                    S.show = "all";
                    showSeg.set("all");
                    keepUrl();
                    paintGroups();
                  },
                },
                "Clear the search",
              ),
            ),
          ]),
    );
    found.textContent = foundText(total, pg.fields.length, narrowed);
    const unknown = pg.unknown ?? [];
    hsDesc.textContent = `Every key of ${pg.path}. Checked with the host's own start-up validation before anything is written.${unknown.length ? ` Keys the host does not read: ${unknown.join(", ")}.` : ""}`;
  };

  /** @param {HostField} f */
  const openKey = (f) => {
    S.editing = f.key;
    S.err = null;
    S.open.add(f.group);
    paintGroups();
    document.getElementById(`k-${f.key}`)?.scrollIntoView({ block: "nearest" });
  };

  // ── loading ───────────────────────────────────────────────────────────
  const loadHost = async () => {
    const r = await fetchJson(
      "/data/host-settings",
      "the host settings",
      abort.signal,
    );
    if (!r.ok) {
      groupBox.replaceChildren(failBox(r.error, () => void loadHost()));
      found.textContent = "";
      return;
    }
    const first = page === null;
    page = r.body.page;
    if (first && page)
      for (const g of groupsOf(page.fields).slice(0, 2)) S.open.add(g);
    // A staged change whose key the new read no longer has is dropped.
    for (const k of [...changes.keys()])
      if (!page?.fields.some((f) => f.key === k)) changes.delete(k);
    paintSide(spyCurrent);
    paint();
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
    if (first) goToSection();
  };

  let wentTo = false;
  const goToSection = () => {
    if (wentTo || !section) return;
    const id = sectionTarget(section, page ? groupsOf(page.fields) : []);
    if (!id) return;
    if (id !== "wc" && id !== "signin" && !page) return;
    wentTo = true;
    const g = page ? groupsOf(page.fields).find((x) => slug(x) === id) : null;
    if (g && !S.open.has(g)) {
      S.open.add(g);
      paintGroups();
    }
    requestAnimationFrame(() =>
      document.getElementById(id)?.scrollIntoView({ block: "start" }),
    );
  };

  const loadRepo = async () => {
    const r = await fetchJson("/data/repo", "the working copy", abort.signal);
    if (!r.ok) {
      wcBody.replaceChildren(failBox(r.error, () => void loadRepo()));
      return;
    }
    drawRepo(r.body);
  };

  /**
   * @param {{repo: {present: boolean, remote: string, branch: string,
   *   head: CommitRef | null, unpushed: CommitRef[], behind: number,
   *   dirty: string[], key_present: boolean | null, fetched_at: number | null,
   *   error: string | null}, deployed_unpushed: {stack: string, commit: string}[]}} v
   */
  const drawRepo = (v) => {
    const r = v.repo;
    const st = repoState(r);
    const fetched = agoEl("Last fetched", r.fetched_at ?? null);
    wcFoot.replaceChildren(
      el("span", null, r.fetched_at ? fetched : "Not fetched yet"),
      el(
        "span",
        null,
        "Push, rebase or drop appear here when the remote lacks commits",
      ),
    );
    /** @type {Node[]} */
    const alerts = [];
    if (r.error)
      alerts.push(
        el(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          `The last fetch or clone failed: ${r.error}`,
        ),
      );
    if (r.dirty.length)
      alerts.push(
        el(
          "div",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          `Files differ from the last commit: ${r.dirty.join(", ")}. Edits are refused until that is looked at by hand.`,
        ),
      );
    if (r.unpushed.length) {
      const deployed = v.deployed_unpushed.map((d) => d.stack);
      alerts.push(
        el(
          "div",
          { class: "kp-alert kp-alert--warning", role: "status" },
          el(
            "div",
            { class: "kp-alert__body" },
            el(
              "strong",
              null,
              `${r.unpushed.length} commit(s) here are not on the remote. `,
            ),
            deployed.length
              ? `The host runs ${deployed.join(", ")} from one of them: push keeps the repository and the fleet in step.`
              : "Push them, rebase them onto the remote, or drop them.",
            el(
              "ul",
              null,
              r.unpushed.map((c) =>
                el(
                  "li",
                  { class: "cf-mono" },
                  `${c.commit.slice(0, 10)} · ${c.subject}`,
                ),
              ),
            ),
            el(
              "div",
              { class: "cf-row" },
              /** @type {const} */ (["push", "rebase", "drop"]).map((choice) =>
                resolveButton(choice, loadRepo),
              ),
            ),
          ),
        ),
      );
    }
    wcBody.replaceChildren(
      el(
        "div",
        { class: "st-wc__grid" },
        el(
          "div",
          { class: "st-wc__commit" },
          el("span", { class: "cf-hint" }, "At"),
          el(
            "strong",
            null,
            r.head ? r.head.subject : r.present ? "—" : "not cloned yet",
          ),
          el(
            "span",
            { class: "cf-row" },
            r.head
              ? el(
                  "span",
                  { class: "cf-chip cf-chip--mono", title: r.head.commit },
                  r.head.commit.slice(0, 10),
                )
              : null,
            chip(r.branch),
            el(
              "span",
              { class: "cf-mono cf-muted st-remote", title: r.remote },
              shortRemote(r.remote),
            ),
          ),
        ),
        el(
          "div",
          { class: "st-wc__chips" },
          dot(st.tone, st.word),
          el(
            "span",
            {
              class: "cf-chip",
              title: "Commits on the remote the host has not fetched",
            },
            `${r.behind} behind`,
          ),
          el(
            "span",
            {
              class: "cf-chip",
              title: "Commits here the remote lacks: push, rebase or drop them",
            },
            `${r.unpushed.length} unpushed`,
          ),
          el(
            "span",
            { class: "cf-chip", title: "Files changed but not committed" },
            `${r.dirty.length} uncommitted`,
          ),
          r.key_present === null
            ? null
            : el(
                "span",
                {
                  class: `cf-chip${r.key_present ? "" : " cf-chip--warn"}`,
                  title: "The deploy key the dashboard pushes with",
                },
                r.key_present ? "deploy key present" : "deploy key missing",
              ),
        ),
      ),
      ...alerts,
    );
  };

  fetchNow.addEventListener("click", async () => {
    /** @type {HTMLButtonElement} */ (fetchNow).disabled = true;
    const x = await send(
      "POST",
      "/data/repo/sync",
      undefined,
      "the working copy",
    );
    /** @type {HTMLButtonElement} */ (fetchNow).disabled = false;
    if (!x.ok) await refusalAlarm(x.error, x.status);
    else notify("The working copy is up to date.", "success");
    void loadRepo();
  });

  // ── fix-120: per-machine tokens ──
  /** @param {{name: string, scope: string}[]} tokens */
  const paintTokens = (tokens) => {
    if (!tokens.length) {
      tokenList.replaceChildren(
        el(
          "div",
          { class: "cf-empty" },
          el("strong", null, "No token yet"),
          el("span", null, "Issue token… adds one for a machine."),
        ),
      );
      return;
    }
    tokenList.replaceChildren(
      el(
        "ul",
        { class: "si-items" },
        tokens.map((v) => {
          const revoke = /** @type {HTMLButtonElement} */ (
            drivable(
              el(
                "button",
                {
                  type: "button",
                  class: "kp-button kp-button--sm kp-button--ghost si-danger",
                  "data-token": v.name,
                  title: `Revoke ${v.name}'s token: that machine can no longer reach the host`,
                },
                "Revoke",
              ),
              REVOKE_TOKEN,
              v.name,
            )
          );
          if (v.name === "legacy") {
            revoke.disabled = true;
            revoke.title =
              "The single `token` key is cleared over ssh (see the migration note in the runbook)";
          } else
            revoke.addEventListener("click", () => revokeTokenDialog(v.name));
          return el(
            "li",
            { class: "si-item" },
            el(
              "div",
              { class: "si-item__id" },
              el("strong", null, v.name),
              el(
                "span",
                { class: "cf-hint" },
                v.name === "legacy"
                  ? "the single host.toml token every machine shared"
                  : "its own bearer, hashed at rest",
              ),
            ),
            chip(v.scope),
            revoke,
          );
        }),
      ),
    );
  };
  const loadTokens = async () => {
    const r = await fetchJson("/data/tokens", "the tokens", abort.signal);
    if (!r.ok) {
      tokenList.replaceChildren(failBox(r.error, () => void loadTokens()));
      return;
    }
    paintTokens(r.body.tokens ?? []);
  };
  const issueTokenDialog = () => {
    const name = el("input", {
      class: "kp-field__input",
      type: "text",
      id: "token-name",
      autocomplete: "off",
      placeholder: "e.g. wsl, ct120-dev",
    });
    const scope = /** @type {HTMLSelectElement} */ (
      el(
        "select",
        { class: "kp-field__input", id: "token-scope" },
        el("option", { value: "read" }, "read — look only"),
        el("option", { value: "operate" }, "operate — act without destroying"),
        el(
          "option",
          { value: "all" },
          "all — everything, including the dashboard's own",
        ),
      )
    );
    scope.value = "operate";
    const go = /** @type {HTMLButtonElement} */ (
      el(
        "button",
        { type: "button", class: "kp-button kp-button--primary" },
        "Issue",
      )
    );
    const cancel = el(
      "button",
      { type: "button", class: "kp-button" },
      "Cancel",
    );
    const errBox = el("div");
    const d = openDialog({
      title: "Issue a new token",
      description:
        "What is it for, and what may it do? The plaintext is shown once, right after this — save it there; the host keeps only its SHA-256.",
      id: "token-issue-dialog",
      body: [
        el(
          "div",
          { class: "kp-field" },
          el("label", { class: "kp-field__label", for: "token-name" }, "Name"),
          name,
          el(
            "span",
            { class: "kp-field__help" },
            "What `homelab doctor` and audit.log will call this machine.",
          ),
        ),
        el(
          "div",
          { class: "kp-field" },
          el(
            "label",
            { class: "kp-field__label", for: "token-scope" },
            "Scope",
          ),
          scope,
        ),
        errBox,
        el("div", { class: "kp-dialog__actions" }, cancel, go),
      ],
    });
    const unregister = register("tokens-issue", {
      dialog: d.dialog,
      go: () => go,
      cancel: () => cancel,
      close: () => d.close(),
    });
    d.closed.then(unregister);
    cancel.addEventListener("click", () => d.close());
    go.addEventListener("click", async () => {
      if (driven()) return;
      const n = /** @type {HTMLInputElement} */ (name).value.trim();
      if (!n) {
        markErrors(new Map([["name", name]]), { name: "A name is needed." });
        return;
      }
      go.disabled = true;
      const r = await send(
        "POST",
        "/data/tokens",
        { name: n, scope: scope.value },
        "a new token",
      );
      go.disabled = false;
      if (!r.ok) {
        errBox.replaceChildren(refusalCallout(r.error));
        return;
      }
      d.close();
      showIssuedToken(r.body.issued);
      void loadTokens();
    });
  };
  /** @param {{name: string, scope: string, token: string}} issued */
  const showIssuedToken = (issued) => {
    const box = /** @type {HTMLInputElement} */ (
      el("input", {
        class: "kp-field__input cf-mono",
        type: "text",
        readonly: true,
        id: "issued-token",
      })
    );
    box.value = issued.token;
    const copy = el("button", { type: "button", class: "kp-button" }, "Copy");
    copy.addEventListener("click", async () => {
      try {
        await navigator.clipboard.writeText(issued.token);
        notify("Copied.", "success");
      } catch {
        box.select();
      }
    });
    const done = el(
      "button",
      { type: "button", class: "kp-button kp-button--primary" },
      "Done — I saved it",
    );
    const d = openDialog({
      title: `${issued.name} · shown once`,
      description: `Set it as HOMELAB_TOKEN on that machine (its own .env or ~/.config/homelab/env). It is never shown again, and it is not shared with any other machine's token.`,
      id: "token-issued-dialog",
      body: [
        el("div", { class: "kp-field" }, box, " ", copy),
        el("div", { class: "kp-dialog__actions" }, done),
      ],
    });
    done.addEventListener("click", () => d.close());
  };
  const revokeTokenDialog = (/** @type {string} */ name) => {
    const typed = /** @type {HTMLInputElement} */ (
      el("input", {
        class: "kp-field__input",
        type: "text",
        id: "token-revoke-confirm",
        autocomplete: "off",
        placeholder: name,
      })
    );
    const go = /** @type {HTMLButtonElement} */ (
      el(
        "button",
        { type: "button", class: "kp-button kp-button--destructive" },
        "Revoke",
      )
    );
    const cancel = el(
      "button",
      { type: "button", class: "kp-button" },
      "Cancel",
    );
    const errBox = el("div");
    const d = openDialog({
      title: `Revoke ${name}`,
      description:
        "That machine's token stops working at once; no other machine's token is affected.",
      id: "token-revoke-dialog",
      body: [
        el(
          "div",
          { class: "kp-field" },
          el(
            "label",
            { class: "kp-field__label", for: "token-revoke-confirm" },
            `Type ${name} to confirm`,
          ),
          typed,
        ),
        errBox,
        el("div", { class: "kp-dialog__actions" }, cancel, go),
      ],
    });
    cancel.addEventListener("click", () => d.close());
    go.addEventListener("click", async () => {
      if (driven()) return;
      if (typed.value.trim() !== name) {
        markErrors(new Map([["c", typed]]), { c: `Type ${name} exactly.` });
        return;
      }
      go.disabled = true;
      const r = await send(
        "DELETE",
        `/data/tokens/${encodeURIComponent(name)}`,
        undefined,
        "revoke a token",
      );
      go.disabled = false;
      if (!r.ok) {
        errBox.replaceChildren(refusalCallout(r.error));
        return;
      }
      notify(`${name} revoked.`, "success");
      d.close();
      void loadTokens();
    });
  };
  issueBtn.addEventListener("click", () => issueTokenDialog());

  // ── review and write ───────────────────────────────────────────────────
  const openReview = () => {
    if (!page || !changes.size) return;
    const body = hostSettingsBody(page.sha256, changes, confirmed);
    // Owner decision 2026-09-30 (item 2): a staged change whose key only
    // takes effect at the host's next start means one press both saves
    // and restarts homelab-host.service, so the change is in force at
    // once instead of waiting for the next unrelated restart.
    const restartNeeded = [...changes.values()].some(
      ({ field }) => field.apply === "restart",
    );
    const list = el(
      "ul",
      { class: "batch-preview" },
      [...changes.values()].map(({ field, value }) =>
        el(
          "li",
          null,
          el("strong", null, field.key),
          ` ${valueText(field)} → ${pendingText(field, value)} · ${field.apply === "live" ? "at once" : "at the host's next start"}`,
        ),
      ),
    );
    const runError = el("div");
    const jobBox = el("div");
    const save = /** @type {HTMLButtonElement} */ (
      el(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          id: "settings-write",
        },
        restartNeeded ? "Save and restart the host" : "Save",
      )
    );
    const cancel = el("button", { type: "button", class: "kp-button" }, "Back");
    const d = openDialog({
      title: restartNeeded
        ? "Write host.toml and restart the host"
        : "Write host.toml",
      description: restartNeeded
        ? "The host checks the change like a start and refuses it whole if anything is wrong, or if host.toml changed since this page read it. At least one of these keys only takes effect at the host's next start, so this also restarts homelab-host.service (refused while another job runs) and waits for it to answer again."
        : "The host checks the change like a start and refuses it whole if anything is wrong, or if host.toml changed since this page read it.",
      id: "settings-review",
      wide: true,
      body: [
        list,
        runError,
        jobBox,
        el("div", { class: "kp-dialog__actions" }, cancel, save),
      ],
    });
    /** @type {() => void} */
    let stopPanel = () => {};
    const unregisterReview = register("host-review", {
      dialog: d.dialog,
      save: () => save,
      back: () => cancel,
      runError: (/** @type {import("../doctor.js").RouteError | null} */ e) =>
        runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
      close: () => d.close(),
    });
    d.closed.then(() => {
      unregisterReview();
      stopPanel();
    });
    cancel.addEventListener("click", () => d.close());
    save.addEventListener("click", async () => {
      // Live view: host.toml is written on the dashboard's server, once,
      // and the restart follow-up (below) is queued there too.
      if (driven()) return;
      save.disabled = true;
      const r = await send(
        "PUT",
        "/data/host-settings",
        body,
        "the host settings",
      );
      if (!r.ok) {
        save.disabled = false;
        runError.replaceChildren(refusalCallout(r.error));
        await refusalAlarm(r.error, r.status);
        return;
      }
      saved(r.body);
      if (r.body.follow?.refused) {
        runError.replaceChildren(
          refusalCallout(r.body.follow.refused, "warning", "Not restarted"),
        );
        save.disabled = false;
        return;
      }
      const job = r.body.follow?.job;
      if (typeof job !== "number") {
        d.close();
        return;
      }
      cancel.hidden = true;
      save.hidden = true;
      const panel = mountJobPanel(job, { compact: true });
      stopPanel = panel.stop;
      jobBox.replaceChildren(panel.element);
    });
  };

  /** host.toml was written: say so, and read it again.
   * @param {any} answer */
  const saved = (answer) => {
    const s = answer.saved ?? {};
    const later = /** @type {string[]} */ (s.restart ?? []);
    const restarting = typeof answer.follow?.job === "number";
    notify(
      restarting
        ? "host.toml written; restarting homelab-host.service…"
        : `host.toml written.${later.length ? ` At the host's next start: ${later.join(", ")}.` : " In force now."}`,
      "success",
    );
    changes.clear();
    confirmed.clear();
    paint();
    void loadHost();
  };

  // feat-platform-10: the Live view replay works this very page: the
  // changes Claude kept on the dashboard's server, the inline key editor
  // and the review dialog are the ones Edit and Check and write open.
  let lastSaved = "";
  const unregister = register("host-settings", {
    ready: () => page !== null,
    openKey: (/** @type {string} */ key) => {
      const f = page?.fields.find((x) => x.key === key);
      if (f) openKey(f);
    },
    rowButton: (/** @type {string} */ key) => {
      const f = page?.fields.find((x) => x.key === key);
      if (f && !S.open.has(f.group)) {
        S.open.add(f.group);
        paintGroups();
      }
      return /** @type {HTMLElement | null} */ (
        groupBox.querySelector(`button[data-key="${CSS.escape(key)}"]`)
      );
    },
    reviewButton: () => (write.disabled ? null : write),
    setStaged: (
      /** @type {Record<string, unknown>} */ want,
      /** @type {string[]} */ typed,
    ) => {
      if (!page) return;
      const same =
        changes.size === Object.keys(want).length &&
        [...changes].every(
          ([k, v]) => JSON.stringify(v.value) === JSON.stringify(want[k]),
        );
      if (same) return;
      changes.clear();
      confirmed.clear();
      for (const [k, v] of Object.entries(want)) {
        const field = page.fields.find((x) => x.key === k);
        if (field) changes.set(k, { field, value: v });
      }
      for (const k of typed) confirmed.add(k);
      paint();
    },
    openReview: () => openReview(),
    showSaved: (/** @type {any} */ answer) => {
      const key = JSON.stringify(answer);
      if (key === lastSaved) return;
      lastSaved = key;
      saved(answer);
    },
  });

  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (
      e.key === "Escape" &&
      S.editing &&
      !document.querySelector("dialog[open]")
    )
      closeEditor();
  };
  document.addEventListener("keydown", onKey);

  paint();
  goToSection();
  const offRepo = listen("repo", (v) => drawRepo(v));
  const offHost = listen("host_settings", () => void loadHost());
  const offTokens = listen("tokens", () => void loadTokens());
  void loadRepo().catch(() => {});
  void loadHost().catch(() => {});
  void loadTokens().catch(() => {});
  return () => {
    unregister();
    unregisterKey?.();
    abort.abort();
    offRepo();
    offHost();
    offTokens();
    hideTip();
    document.removeEventListener("keydown", onKey);
    for (const s of stops) s();
    root.classList.remove("cf-page", "st-page");
  };
}

/**
 * `el` that is also a declared Live view control (on `row` when it repeats).
 * @param {string} tag
 * @param {string} id
 * @param {string | null} row
 * @param {Record<string, any> | null} attrs
 * @param {...import("./configkit.js").Child} kids
 */
function drive(tag, id, row, attrs, ...kids) {
  return drivable(el(tag, attrs, ...kids), id, row ?? undefined);
}

/**
 * A card as the demo draws it: heading + one sentence left, tools right,
 * the body, an optional footer line.
 * @param {{id: string, title: string, desc: string, tools?: Node[],
 *   body: Node[], foot?: Node}} spec
 */
function card(spec) {
  return el(
    "section",
    {
      class: "kp-card nx-card cf-card",
      id: spec.id,
      "aria-labelledby": `${spec.id}-h`,
    },
    el(
      "div",
      { class: "nx-card__head" },
      el("h2", { id: `${spec.id}-h` }, spec.title),
      el("p", { class: "section-head__desc" }, spec.desc),
      spec.tools?.length
        ? el("div", { class: "nx-card__tools" }, spec.tools)
        : null,
    ),
    el("div", { class: "nx-card__body" }, spec.body),
    spec.foot ?? null,
  );
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
function resolveButton(choice, reload) {
  const b = el(
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
      const typed = /** @type {HTMLInputElement} */ (
        el("input", {
          class: "kp-field__input",
          type: "text",
          id: "drop-confirm",
          placeholder: "drop",
        })
      );
      const go = el(
        "button",
        { type: "button", class: "kp-button kp-button--destructive" },
        "Drop the commits",
      );
      const d = openDialog({
        title: "Drop the unpushed commits",
        description:
          "The working copy goes back to the remote's branch; those commits are gone from here.",
        id: "drop-dialog",
        body: [
          el(
            "div",
            { class: "kp-field" },
            el(
              "label",
              { class: "kp-field__label", for: "drop-confirm" },
              "Type drop to confirm",
            ),
            typed,
          ),
          el("div", { class: "kp-dialog__actions" }, go),
        ],
      });
      go.addEventListener("click", async () => {
        if (typed.value.trim() !== "drop") {
          markErrors(new Map([["c", typed]]), { c: "Type drop exactly." });
          return;
        }
        d.close();
        await resolve(choice, reload);
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
    { choice },
    "the unpushed commits",
  );
  if (!r.ok) await refusalAlarm(r.error, r.status);
  else notify(choice === "drop" ? "Dropped." : "Pushed.", "success");
  void reload();
}
