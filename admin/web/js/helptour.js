// redesign-flows-3 (redesign 3.71.0, FLOWS.md §1.2 "Help" and "Tour",
// demo flows/shell.js; Kenny approved 2026-10-03): the Help panel behind
// the bar's "?" — where things live (the six areas, one line each), the
// words (one verb, one meaning), the keys — and "Take the 1-minute tour":
// six steps that point at the search box and the areas, shown on a first
// visit and replayable from Help. A beginner looks a word up where they
// are, and is shown around once.
//
// The shell's one hook: chrome.js builds its "?" sheet with `helpPanel()`
// (the `data-kp-shortcuts` dialog kp's palette opens on "?" and on the
// bar's button), and the panel itself starts the first-visit tour once the
// bar is drawn. The tour's pure half (`tourSteps`, `shouldTour`) is held
// by node tests.

import { AREAS } from "./areas.js";
import { ensureStyle, h } from "./dom.js";
import { declare, dialogControl } from "./drivable.js";
import {
  shouldStartTour,
  startTour as startKpTour,
  tourRemembered,
} from "/static/kp/js/tour.js";

/** Review item 15: the bar's "?" (chrome.js), a Live view control on every
 * page — `bar`: found in the bar, never by going to a page first. */
export const HELP_OPEN = declare({
  id: "help-open",
  page: "home",
  bar: true,
  opens: "dialog",
  what: "open Help: the six areas, the words, the keys and the tour",
});

/** The name kp's tour remembers itself under (`kp-remember:tour:homelab:done`). */
export const TOUR_NAME = "homelab";

/**
 * The key this dashboard used before kp's tour.js remembered it. Read once, so
 * a browser that has seen the tour does not get it again; never written.
 */
export const LEGACY_TOURED_KEY = "homelab-toured";

/** The words the Help panel explains (FLOWS.md §1.2, §1.4). */
export const GLOSSARY = /** @type {const} */ ([
  [
    "Stack",
    "One container on the host with the apps it runs, described by files in the repository.",
  ],
  [
    "App",
    "One program inside a stack, usually a Docker container (traefik, a film server…).",
  ],
  [
    "Deploy",
    "Make the stack match its files: create, change or restart what differs.",
  ],
  [
    "Update",
    "Move an app to a newer image. Always backed up first; Roll back puts the old version back.",
  ],
  [
    "Back up / snapshot",
    "A dated copy of an app's data (restic). One every night, plus whenever you press Back up.",
  ],
  [
    "Restore",
    "Put an app's data back as it was in a snapshot. A safety copy of today's data is made first.",
  ],
  [
    "Secret",
    "A password or token a stack needs; stored sealed (latch), never shown until you press Reveal.",
  ],
  [
    "Job",
    "One action running on the host. Follow it from the running pill, or in Activity.",
  ],
  [
    "Park",
    "Take a stack out of the nightly round and start-on-boot; nothing is stopped.",
  ],
]);

/**
 * @typedef {{desktop: string, phone: string, title: string, text: string}} TourStep
 *   `desktop` / `phone`: the selector of what the step points at, at each
 *   width (the bar's links, or the phone's tab bar and its More)
 */

/**
 * The six steps (FLOWS.md §1.2 "Tour"): search, then the areas in the
 * order the bar shows them.
 * @returns {TourStep[]}
 */
export function tourSteps() {
  const area = (/** @type {string} */ id) => AREAS.find((a) => a.id === id);
  const nav = (/** @type {string} */ id) => `#nav a[data-area="${id}"]`;
  const tab = (/** @type {string} */ id) =>
    `.kp-tabbar [data-kp-tab="${area(id)?.phone === "tab" ? id : "more"}"]`;
  return [
    {
      desktop: ".nx-search-trigger",
      phone: ".nx-search-trigger",
      title: "One box for everything",
      text: "Type what you want — “update gateway”, “restore notes”, “backups”. Actions open their own dialog; nothing runs until you confirm.",
    },
    {
      desktop: nav("needs-you"),
      phone: tab("needs-you"),
      title: area("needs-you")?.label ?? "Inbox",
      text: "Start here when something is off. Every problem, question and available update in one list, worst first, each with the button that fixes it.",
    },
    {
      desktop: nav("overview"),
      phone: tab("overview"),
      title: area("overview")?.label ?? "Stacks",
      text: "Every stack and its state. Open one to get its hub: status, logs, backups, history and settings together.",
    },
    {
      desktop: nav("activity"),
      phone: tab("activity"),
      title: area("activity")?.label ?? "Activity",
      text: "What runs now, what ran, and who ran it — you, Claude, a schedule or the nightly round.",
    },
    {
      desktop: nav("backups"),
      phone: tab("backups"),
      title: area("backups")?.label ?? "Backups",
      text: "Every snapshot of every night, and Restore. All backup matters live here.",
    },
    {
      desktop: nav("system"),
      phone: tab("system"),
      title: area("system")?.label ?? "System",
      text: "The host, metrics, the map of the fleet and the settings. You rarely need it.",
    },
  ];
}

/**
 * Whether this page load starts the tour on its own: `?tour` asks for it;
 * otherwise only on a first visit, and never for an automated browser (the
 * whole-screen tests, Live view's own tab), which would get it on every
 * fresh profile. kp's `shouldStartTour`, with this dashboard's word for it.
 * @param {{search: string, toured: boolean, automated: boolean}} s
 * @returns {number | null} the step to start at (0-based), or null
 */
export function shouldTour(s) {
  return shouldStartTour({
    search: s.search,
    remembered: s.toured,
    automated: s.automated,
  });
}

/** Has this browser seen the tour, under kp's key or the old one? */
function toured() {
  if (tourRemembered(TOUR_NAME)) return true;
  try {
    return localStorage.getItem(LEGACY_TOURED_KEY) != null;
  } catch {
    return false;
  }
}

/**
 * Show the tour from step `i` (FLOWS.md §1.2): kp's tour card beside what
 * it points at, Back / Skip / Next, "n of 6" counted from the steps that
 * are on screen; Esc or Skip ends it, and the browser remembers it was seen.
 * @param {number} [i]
 */
export function startTour(i = 0) {
  const phone = matchMedia("(max-width: 60rem)").matches;
  startKpTour(
    tourSteps().map((s) => ({
      target: phone ? s.phone : s.desktop,
      title: s.title,
      text: s.text,
    })),
    {
      start: i,
      remember: TOUR_NAME,
      // The tour starts from the Help panel or on a first visit, where
      // nothing holds focus: it ends on the bar's "?" that opens Help.
      returnFocus: document.querySelector(".help-button"),
      // Live view finds the card's buttons by name.
      decorate: (part, info) => void dialogControl(part, info.kind),
    },
  );
}

/**
 * A card of the Help panel, kp-themes' `.kp-help`: heading, one sentence,
 * then its list of terms beside their meanings.
 */
function card(
  /** @type {string} */ title,
  /** @type {string} */ desc,
  /** @type {Node} */ body,
) {
  return h(
    "section",
    { class: "kp-help kp-card" },
    h("h3", null, title),
    h("p", null, desc),
    body,
  );
}

/** @param {[Node | string, Node | string][]} rows */
const kv = (rows) =>
  h(
    "dl",
    { class: "kp-help__list" },
    ...rows.flatMap(([k, v]) => [h("dt", null, k), h("dd", null, v)]),
  );

/**
 * The Help panel (FLOWS.md §1.2): a side drawer kp's palette opens on "?"
 * and on the bar's "?" button (`data-kp-shortcuts`, id `shortcuts`). It
 * also starts the first-visit tour once the bar is drawn.
 * @returns {HTMLDialogElement}
 */
export function helpPanel() {
  ensureHelpStyle();
  const close = h(
    "button",
    {
      type: "button",
      class: "kp-icon-button kp-dialog__close",
      "aria-label": "Close",
      title: "Close Help (Esc)",
    },
    "✕",
  );
  const tour = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary help-tour",
      title: "Six short steps that show where everything lives",
    },
    "Take the 1-minute tour",
  );
  dialogControl(tour, "take-the-tour");
  // feat-kp11demo-14: the keys are not typed out here. kp's shortcut engine
  // writes this body from the keys it handles, so the sheet cannot promise
  // a key that does nothing; a key that does nothing on this page is dimmed
  // with its reason (js/shortcuts.js, chrome.js).
  const keys = card(
    "Keys",
    "Optional; every key also has a button.",
    h("div", { "data-kp-shortcuts-body": "" }),
  );
  const d = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      {
        class: "kp-drawer kp-dialog help-panel",
        id: "shortcuts",
        "data-kp-shortcuts": "",
        "data-kp-shortcuts-live": "",
        "aria-label": "Help, words and shortcuts",
      },
      h(
        "header",
        { class: "kp-drawer__head" },
        h("h2", { class: "kp-dialog__title" }, "Help"),
        close,
        h(
          "p",
          { class: "kp-drawer__desc" },
          "What the words mean, where things live, and the keys.",
        ),
      ),
      h(
        "div",
        { class: "kp-drawer__body" },
        card(
          "Where things live",
          "Six areas, left to right in the order you usually need them.",
          kv(
            AREAS.map((a) => [
              a.label,
              h(
                "span",
                null,
                a.what,
                " · ",
                dialogControl(
                  h("a", { href: a.href, title: `Go to ${a.label}` }, "open"),
                  `open-${a.id}`,
                ),
              ),
            ]),
          ),
        ),
        card(
          "Words",
          "The same word means the same thing on every page.",
          kv(GLOSSARY.map(([w, m]) => [w, m])),
        ),
        keys,
      ),
      h("footer", { class: "kp-drawer__foot" }, tour),
    )
  );
  dialogControl(close, "close");
  close.addEventListener("click", () => d.close());
  tour.addEventListener("click", () => {
    d.close();
    startTour(0);
  });
  // A link inside Help navigates (the app's router takes it); the panel
  // closes with it.
  d.addEventListener("click", (e) => {
    if (/** @type {Element} */ (e.target).closest("a[href]")) d.close();
  });
  // The first visit's tour, once the bar is drawn.
  setTimeout(() => {
    const at = shouldTour({
      search: location.search,
      toured: toured(),
      automated: navigator.webdriver === true,
    });
    if (at != null) startTour(at);
  }, 900);
  return d;
}

/** The panel's and the tour's own styles (css/pages/help.css). */
const ensureHelpStyle = () => ensureStyle("/css/pages/help.css");
