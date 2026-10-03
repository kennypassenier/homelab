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
import { SHORTCUTS } from "./shortcuts.js";
import { declare, dialogControl } from "./drivable.js";
import { isMac } from "/static/kp/js/palette.js";

/** Review item 15: the bar's "?" (chrome.js), a Live view control on every
 * page — `bar`: found in the bar, never by going to a page first. */
export const HELP_OPEN = declare({
  id: "help-open",
  page: "home",
  bar: true,
  opens: "dialog",
  what: "open Help: the six areas, the words, the keys and the tour",
});

/** Remembers that this browser has seen (or skipped) the tour. */
export const TOURED_KEY = "homelab-toured";

/** The words the Help panel explains (FLOWS.md §1.2, §1.4). */
export const GLOSSARY = /** @type {const} */ ([
  [
    "Stack",
    "One container on the host with the apps it runs, described by files in the repository.",
  ],
  [
    "App",
    "One program inside a stack, usually a Docker container (traefik, jellyfin…).",
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
    `.nx-tabbar [data-area="${area(id)?.phone === "tab" ? id : "more"}"]`;
  return [
    {
      desktop: ".nx-search-trigger",
      phone: ".nx-search-trigger",
      title: "One box for everything",
      text: "Type what you want — “update gateway”, “restore notes”, “backups”. Actions open their own dialog; nothing runs until you confirm.",
    },
    {
      desktop: nav("inbox"),
      phone: tab("inbox"),
      title: area("inbox")?.label ?? "Inbox",
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
 * fresh profile.
 * @param {{search: string, toured: boolean, automated: boolean}} s
 * @returns {number | null} the step to start at (0-based), or null
 */
export function shouldTour(s) {
  const q = new URLSearchParams(s.search);
  if (q.has("tour")) return Math.max(0, (Number(q.get("tour")) || 1) - 1);
  if (s.toured || s.automated) return null;
  return 0;
}

/** @param {string} key @returns {string | null} */
function stored(key) {
  try {
    return localStorage.getItem(key);
  } catch {
    return null;
  }
}

/** @param {string} key @param {string} value */
function store(key, value) {
  try {
    localStorage.setItem(key, value);
  } catch {
    /* a private window: the tour simply comes again */
  }
}

/** The element a step points at, if it is on screen. @param {string} sel */
function visible(sel) {
  for (const el of document.querySelectorAll(sel)) {
    const r = el.getBoundingClientRect();
    if (r.width > 0 && r.height > 0) return /** @type {HTMLElement} */ (el);
  }
  return null;
}

/** @type {(() => void) | null} */
let endTour = null;

/**
 * Show the tour from step `i` (FLOWS.md §1.2): a card under what it points
 * at, Next / Skip, "n of 6"; Esc or Skip ends it, and the browser
 * remembers it was seen.
 * @param {number} [i]
 */
export function startTour(i = 0) {
  endTour?.();
  const steps = tourSteps();
  const phone = matchMedia("(max-width: 60rem)").matches;
  /** @type {HTMLElement | null} */
  let target = null;
  /** @type {HTMLElement | null} */
  let box = null;
  const clear = () => {
    box?.remove();
    target?.classList.remove("tour-target");
    box = null;
    target = null;
  };
  const finish = () => {
    clear();
    document.removeEventListener("keydown", onKey, true);
    removeEventListener("resize", place);
    endTour = null;
    store(TOURED_KEY, "1");
  };
  const place = () => {
    if (!box || !target) return;
    const r = target.getBoundingClientRect();
    const w = box.offsetWidth;
    const hgt = box.offsetHeight;
    const left = Math.max(
      16,
      Math.min(innerWidth - w - 16, r.left + r.width / 2 - w / 2),
    );
    const top =
      r.top > innerHeight / 2 ? Math.max(16, r.top - hgt - 12) : r.bottom + 12;
    box.style.left = `${left}px`;
    box.style.top = `${top}px`;
  };
  /** @param {number} n */
  const show = (n) => {
    clear();
    if (n >= steps.length) {
      finish();
      return;
    }
    const s = steps[n];
    target = visible(phone ? s.phone : s.desktop);
    if (!target) {
      show(n + 1);
      return;
    }
    target.classList.add("tour-target");
    const next = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm kp-button--primary",
        title:
          n + 1 === steps.length
            ? "End the tour"
            : "Show the next part of the dashboard",
      },
      n + 1 === steps.length ? "Done" : "Next",
    );
    const skip = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "End the tour; Help (?) can start it again",
      },
      "Skip",
    );
    dialogControl(next, "tour-next");
    dialogControl(skip, "tour-skip");
    next.addEventListener("click", () => show(n + 1));
    skip.addEventListener("click", finish);
    // A non-modal <dialog>: its Next and Skip are dialog controls Live view
    // finds by name, and the page stays usable behind it.
    box = h(
      "dialog",
      // Review item 17: named by its step's title, no aria-live on a
      // dialog (opening it already announces it).
      {
        class: "tour",
        "aria-labelledby": "tour-title",
      },
      h("h3", { id: "tour-title" }, s.title),
      h("p", null, s.text),
      h(
        "footer",
        null,
        h("span", null, `${n + 1} of ${steps.length}`),
        h("span", { class: "tour__btns" }, skip, next),
      ),
    );
    document.body.append(box);
    /** @type {HTMLDialogElement} */ (box).show();
    place();
    next.focus();
  };
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.key !== "Escape" || !box) return;
    e.preventDefault();
    e.stopPropagation();
    finish();
  };
  document.addEventListener("keydown", onKey, true);
  addEventListener("resize", place);
  endTour = finish;
  show(i);
}

/** A card of the Help panel: heading, one sentence, then its list. */
function card(
  /** @type {string} */ title,
  /** @type {string} */ desc,
  /** @type {Node} */ body,
) {
  return h(
    "section",
    { class: "kp-card nx-card help-card" },
    h(
      "div",
      { class: "nx-card__head" },
      h("h3", null, title),
      h("p", { class: "section-head__desc" }, desc),
    ),
    body,
  );
}

/** @param {[Node | string, Node | string][]} rows */
const kv = (rows) =>
  h(
    "dl",
    { class: "help-kv" },
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
      class: "kp-button kp-button--ghost kp-dialog__close",
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
  const keys = SHORTCUTS.map((g) =>
    card(
      g.group,
      "Optional; every key also has a button.",
      kv(
        g.shortcuts.map((s) => [
          h(
            "kbd",
            { class: "nx-kbd" },
            s.keys === "Ctrl K" && isMac() ? "⌘ K" : s.keys,
          ),
          s.what,
        ]),
      ),
    ),
  );
  const d = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      {
        class: "kp-dialog nx-drawer help-panel",
        id: "shortcuts",
        "data-kp-shortcuts": "",
        "aria-label": "Help, words and shortcuts",
      },
      h(
        "div",
        { class: "nx-drawer__head" },
        h("h2", { class: "kp-dialog__title" }, "Help"),
        close,
        h(
          "p",
          { class: "section-head__desc" },
          "What the words mean, where things live, and the keys.",
        ),
      ),
      h(
        "div",
        { class: "nx-drawer__body help-panel__body" },
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
        ...keys,
      ),
      h("div", { class: "nx-drawer__foot" }, tour),
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
      toured: stored(TOURED_KEY) != null,
      automated: navigator.webdriver === true,
    });
    if (at != null) startTour(at);
  }, 900);
  return d;
}

/** The panel's and the tour's own styles (css/pages/help.css). */
const ensureHelpStyle = () => ensureStyle("/css/pages/help.css");
