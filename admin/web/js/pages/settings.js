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
//
// This file is the facade: it builds the page's shared context and mounts
// the sections, each in its own module under settings/ (header, cards,
// workingcopy, hostcard, side, groups, filter, savebar, hostload, livehooks).

import { h } from "../dom.js";
import { listen } from "../store.js";
import { setParams } from "../urlstate.js";
import { ensureStyle } from "../ui.js";
import {
  setCardState,
  setProblems,
  attachContents,
} from "/static/kp/dist/kp-themes.min.js";
import { buildStaticCards } from "./settings/cards.js";
import { buildFailBand, buildHeader, buildSearch } from "./settings/header.js";
import { buildHostCard } from "./settings/hostcard.js";
import { filter } from "./settings/filter.js";
import { focusKey } from "./settings/groups.js";
import { makeLoadHost } from "./settings/hostload.js";
import { registerLiveHooks } from "./settings/livehooks.js";
import { mountSaveBar } from "./settings/savebar.js";
import { buildSide } from "./settings/side.js";
import { buildWorkingCopy, mountWorkingCopy } from "./settings/workingcopy.js";

/**
 * The page's starting state: what the address asks for (`q`, `show`,
 * `section`), nothing read yet.
 * @param {HTMLElement} root
 * @param {{section?: string}} opts
 * @returns {import("./settings/ctx.js").Ctx}
 */
function newContext(root, opts) {
  const q0 = new URLSearchParams(location.search);
  const S = {
    q: (q0.get("q") ?? "").toLowerCase(),
    show: /** @type {import("./settings/ctx.js").Show} */ (
      ["changed", "here"].includes(q0.get("show") ?? "")
        ? q0.get("show")
        : "all"
    ),
  };
  const keepUrl = () =>
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${setParams(location.search, {
        q: S.q,
        show: S.show === "all" ? null : S.show,
      })}`,
    );
  const { band, failed } = buildFailBand();
  return /** @type {import("./settings/ctx.js").Ctx} */ (
    /** @type {unknown} */ ({
      root,
      abort: new AbortController(),
      stops: [],
      S,
      section: opts.section ?? q0.get("section"),
      keepUrl,
      band,
      failed,
      page: null,
      typed: new Set(),
      stale: false,
      detachBar: null,
      saveBtn: null,
      detachRows: null,
      unbridge: null,
    })
  );
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
  const ctx = newContext(root, opts);
  ctx.filter = () => filter(ctx);
  ctx.focusKey = (key) => focusKey(ctx, key);
  Object.assign(ctx, buildHeader(), buildSearch(ctx));
  const { wc, wcBody } = buildWorkingCopy();
  Object.assign(ctx, { wc, wcBody });
  const cards = buildStaticCards(ctx);
  Object.assign(ctx, buildHostCard(ctx));
  ctx.hsCard.card.querySelector(".kp-lede")?.after(ctx.hostBar);
  const { side, paintSide } = buildSide(ctx);

  const main = h(
    "div",
    { class: "st-main" },
    wc.card,
    cards.signin.card,
    cards.home.card,
    cards.accountCard.card,
    cards.browserCard.card,
    ctx.hsCard.card,
  );
  root.replaceChildren(
    ctx.head.el,
    ctx.band,
    ctx.tb.el,
    h("div", { class: "st-split" }, side, main),
  );
  setProblems(ctx.band, []);
  setCardState(ctx.hsCard.card, "loading", { words: "Reading host.toml…" });
  setCardState(wc.card, "loading", { words: "Reading the working copy…" });
  ctx.stops.push(attachContents(side));
  paintSide();

  mountSaveBar(ctx);
  ctx.loadHost = makeLoadHost(ctx, paintSide);
  const { loadVersions, drawRepo } = mountWorkingCopy(ctx);
  const unregister = registerLiveHooks(ctx);
  const offRepo = listen("repo", (v) => drawRepo(v));
  const offHost = listen("host_settings", () => void ctx.loadHost());
  void ctx.loadRepo().catch(() => {});
  void ctx.loadHost().catch((e) => console.error("loadHost", e));
  void loadVersions().catch(() => {});
  return () => {
    unregister();
    ctx.abort.abort();
    offRepo();
    offHost();
    ctx.detachRows?.();
    ctx.unbridge?.();
    ctx.detachBar?.();
    for (const s of ctx.stops) s();
    root.classList.remove("cf-page", "st-page");
  };
}
