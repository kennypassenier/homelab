// feat-kp11demo-5.4 ("This browser"): what only this device decides, in one
// card on Settings: the theme (every theme drawn as its own swatch, light and
// dark apart), what Live view opens in, "follow my system", the
// tour, and a way to forget what this browser remembers. Nothing here is sent
// to the host. The card is a form grid: the name and one sentence in the
// label track, the control in the control track, so every control starts on
// one line.

import { TOUR_NAME, startTour } from "./helptour.js";
import { FOLLOW_KEY } from "./driveview.js";
import { esc } from "./settingsform.js";
import { attachCalm } from "/static/kp/dist/kp-themes.min.js";
import { openConfirmation } from "/static/kp/dist/kp-themes.min.js";
import { REMEMBER_PREFIX } from "/static/kp/dist/kp-themes.min.js";
import { attachSegmented } from "/static/kp/dist/kp-themes.min.js";
import {
  THEMES,
  applyTheme,
  currentTheme,
} from "/static/kp/dist/kp-themes.min.js";
import {
  PICK_EVENT,
  attachThemePickers,
  themeOptionsMarkup,
} from "/static/kp/dist/kp-themes.min.js";
import { forgetTour } from "/static/kp/dist/kp-themes.min.js";
import { toast } from "/static/kp/dist/kp-themes.min.js";

/** Where "follow my system" and the two themes it picks between are kept. */
const FOLLOW_SYSTEM = "homelab.theme-follow-system";
const LIGHT_PICK = "homelab.theme-light";
const DARK_PICK = "homelab.theme-dark";

/** @param {string} key */
const read = (key) => {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
};
/** @param {string} key @param {string | null} value */
const write = (key, value) => {
  try {
    if (value == null) localStorage.removeItem(key);
    else localStorage.setItem(key, value);
  } catch {
    /* storage off: the choice lives while the page does */
  }
};

/**
 * The theme "follow my system" wants now: the light or the dark choice,
 * by what the system prefers. Null when it is off.
 * @param {boolean} dark the system prefers dark
 * @param {{follow: boolean, light: string | null, dark: string | null}} s
 */
export function followedTheme(dark, s) {
  if (!s.follow) return null;
  return (dark ? s.dark : s.light) ?? (dark ? "dark" : "formal");
}

/** Keep the theme in step with the system while "follow" is on. */
export function startFollowingSystem() {
  const mq = matchMedia("(prefers-color-scheme: dark)");
  const apply = () => {
    const want = followedTheme(mq.matches, {
      follow: read(FOLLOW_SYSTEM) === "on",
      light: read(LIGHT_PICK),
      dark: read(DARK_PICK),
    });
    if (want && want !== currentTheme()) applyTheme(want);
  };
  mq.addEventListener("change", apply);
  apply();
  return () => mq.removeEventListener("change", apply);
}

/**
 * The remembered choices this browser holds for the dashboard (kp's
 * `data-kp-remember` memory), without calm mode (it follows the account) and the theme.
 * @returns {string[]} their storage keys
 */
export function rememberedKeys() {
  try {
    return Object.keys(localStorage).filter(
      (k) => k.startsWith(`${REMEMBER_PREFIX}:`) && !k.includes(":calm:"),
    );
  } catch {
    return [];
  }
}

/**
 * @param {string} name @param {string} what @param {string} control
 */
const row = (name, what, control, cls = "") =>
  `<div class="kp-formrow kp-formrow--wide${cls ? ` ${cls}` : ""}"><div class="kp-formrow__label">${esc(name)}<small>${esc(what)}</small></div><div class="kp-formrow__control">${control}</div></div>`;

/** @param {boolean} dark */
const themes = (dark) =>
  `<ul class="st-themes" data-kp-theme-picker aria-label="${dark ? "Dark themes" : "Light themes"}" data-st-kind="${dark ? "dark" : "light"}">${themeOptionsMarkup({ themes: THEMES.filter((t) => !!t.dark === dark), grouped: false })}</ul>`;

/**
 * Draw the Account card's body: the choices that follow Kenny to every device
 * (calm mode; its key syncs, see `store::browser_keys()`).
 * @param {HTMLElement} body the card's stage
 * @returns {() => void} stop
 */
export function mountAccount(body) {
  body.innerHTML = `<div class="kp-formgrid st-lines" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines">${row(
    "Calm mode",
    "Turns movement and effects off; loading keeps breathing slowly. Follows you to every device you sign in on.",
    `<label class="kp-switch"><input class="kp-switch__input" type="checkbox" role="switch" id="st-calm" data-kp-calm-switch /><span>Calm</span></label>`,
  )}</div>`;
  return attachCalm(body);
}

/**
 * Draw the card's body into `body` and wire it.
 * @param {HTMLElement} body the card's stage
 * @returns {() => void} stop
 */
export function mountThisBrowser(body) {
  const live = read(FOLLOW_KEY) === "on" ? "on" : "off";
  const follow = read(FOLLOW_SYSTEM) === "on";
  const sw = (
    /** @type {string} */ id,
    /** @type {boolean} */ on,
    /** @type {string} */ word,
  ) =>
    `<label class="kp-switch"><input class="kp-switch__input" type="checkbox" role="switch" id="${id}"${on ? " checked" : ""} /><span>${word}</span></label>`;
  body.innerHTML = `<div class="kp-formgrid st-lines" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines">${[
    row(
      "Light theme",
      "The look of the dashboard on this device.",
      themes(false),
    ),
    row("Dark theme", "For a dark room, or a dark system.", themes(true)),
    row(
      "Live view opens",
      "Whether Live view follows Claude when this dashboard opens.",
      `<div class="kp-segmented" data-kp-segmented role="radiogroup" aria-label="Live view opens"><button type="button" role="radio" aria-checked="${live === "on"}" data-kp-value="on">Following</button><button type="button" role="radio" aria-checked="${live === "off"}" data-kp-value="off">Off</button></div>`,
    ),
    row(
      "Follow my system",
      "Uses the light theme by day and the dark one by night, as the system decides.",
      sw("st-follow", follow, follow ? "Following" : "Off"),
    ),
  ].join(
    "",
  )}</div><div class="kp-formgrid st-lines" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines">${[
    row(
      "The tour",
      "The short walk over the dashboard.",
      '<button type="button" class="kp-button" id="st-tour">Replay</button>',
      "st-actionrow",
    ),
    row(
      "Remembered here",
      "Open groups, sort orders and folded sections this browser keeps.",
      '<button type="button" class="kp-button kp-button--destructive" id="st-forget">Forget</button>',
      "st-actionrow",
    ),
  ].join("")}</div>`;
  const stops = [attachThemePickers(body), attachSegmented(body)];
  body.addEventListener(PICK_EVENT, (e) => {
    const theme = /** @type {CustomEvent} */ (e).detail?.theme;
    const kind = /** @type {HTMLElement} */ (e.target)
      .closest("[data-st-kind]")
      ?.getAttribute("data-st-kind");
    if (theme && kind) write(kind === "dark" ? DARK_PICK : LIGHT_PICK, theme);
  });
  body.addEventListener("kp-segmented-change", (e) => {
    const v = /** @type {CustomEvent} */ (e).detail?.value;
    if (v === "on" || v === "off") write(FOLLOW_KEY, v);
  });
  const followBox = /** @type {HTMLInputElement} */ (
    body.querySelector("#st-follow")
  );
  followBox.addEventListener("change", () => {
    write(FOLLOW_SYSTEM, followBox.checked ? "on" : "off");
    const word = followBox.parentElement?.querySelector("span");
    if (word) word.textContent = followBox.checked ? "Following" : "Off";
    // The two choices start from what is on now.
    if (followBox.checked) {
      const dark = !!THEMES.find((t) => t.name === currentTheme())?.dark;
      if (!read(dark ? DARK_PICK : LIGHT_PICK))
        write(dark ? DARK_PICK : LIGHT_PICK, currentTheme());
    }
    stopFollow();
    stopFollow = startFollowingSystem();
  });
  let stopFollow = startFollowingSystem();
  body.querySelector("#st-tour")?.addEventListener("click", () => {
    forgetTour(TOUR_NAME);
    startTour(0);
  });
  body.querySelector("#st-forget")?.addEventListener("click", (e) => {
    const keys = rememberedKeys();
    openConfirmation(/** @type {HTMLElement} */ (e.currentTarget), {
      phrase: "Forget what this browser remembers?",
      loses: [
        `${keys.length} remembered ${keys.length === 1 ? "choice" : "choices"}: open groups, sort orders, folded sections`,
        "The tour starts again on the next visit",
        "The theme stays",
      ],
      acceptLabel: "Forget them",
      onAccept: () => {
        for (const k of rememberedKeys()) write(k, null);
        forgetTour(TOUR_NAME);
        toast("This browser forgot what it remembered.", { ms: 4000 });
      },
    });
  });
  return () => {
    stopFollow();
    for (const s of stops) s();
  };
}
