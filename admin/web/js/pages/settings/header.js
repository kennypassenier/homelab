// The top of the Settings page: the alert band for reads that failed, the
// header with the Save bar's home, and the search toolbar.

import { agoEl } from "../../ago.js";
import { drivable } from "../../drivable.js";
import { h } from "../../dom.js";
import { failSummary } from "../../settingsview.js";
import { pageHeader } from "../../ui.js";
import { attachToolbars, setProblems } from "/static/kp/dist/kp-themes.min.js";
import { SEARCH, TRY_AGAIN } from "./controls.js";

/**
 * redesign-config-11: one alert under the header per read the page lost,
 * each with its fix, worst first (kp-themes `problems`).
 * @returns {{band: HTMLElement, failed: import("./ctx.js").Ctx["failed"]}}
 */
export function buildFailBand() {
  /** @type {Map<string, {error: import("../../doctor.js").RouteError, retry: () => void}>} */
  const fails = new Map();
  const band = h("div", {
    class: "kp-problems nx-failband",
    "data-kp-problems": "",
    "data-kp-problems-mode": "each",
  });
  /**
   * @param {string} what
   * @param {import("../../doctor.js").RouteError | null} error
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
  return { band, failed };
}

/**
 * The header: the Save bar IS the page's primary action.
 * @returns {Pick<import("./ctx.js").Ctx, "ago" | "barSlot" | "errBox" | "hostBar"> & {head: {el: HTMLElement}}}
 */
export function buildHeader() {
  const ago = agoEl("read", null, { live: true });
  const barSlot = h("div", {
    class: "kp-unsaved st-unsaved",
    id: "settings-unsaved",
    "data-kp-unsaved-for": "#st-groups",
    "data-kp-name": "settings",
    "data-kp-keep": "session",
  });
  const head = pageHeader({
    title: "Host settings",
    desc: "How this homelab is set up: the working copy it applies from, who can sign in, what only this browser decides, and every key of the host's host.toml. This browser and Account save the moment you change them; host.toml keys wait for Save.",
    titleMeta: [h("span", { class: "cf-live" }, ago)],
  });
  // The Save bar belongs to the host.toml block alone (ux-hostsettings-1):
  // it stands in that card, and the kit docks it at the screen's foot once
  // the card's top has scrolled away.
  const errBox = h("div", { class: "st-saveerr" });
  const hostBar = h(
    "div",
    { class: "st-hostbar" },
    h(
      "p",
      { class: "st-hostbar__note" },
      "Changes to these keys are written to host.toml only when you press Save.",
    ),
    barSlot,
    errBox,
  );
  return { ago, barSlot, head, errBox, hostBar };
}

/**
 * The search over every setting.
 * @param {import("./ctx.js").Ctx} ctx
 */
export function buildSearch(ctx) {
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
  searchBox.value = ctx.S.q;
  searchBox.addEventListener("input", () => {
    ctx.S.q = searchBox.value.trim().toLowerCase();
    ctx.keepUrl();
    ctx.filter();
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
  ctx.stops.push(attachToolbars(tb.el));
  drivable(searchBox, SEARCH);
  return { found, tb };
}
