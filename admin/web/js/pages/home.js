// The Apps page (redesign-stacks, release 3.71.0; rebuilt on the approved
// kp-themes 11 demo, feat-kp11demo-12.4). Home when the Inbox is empty (`/`
// sends there; main.js `landing`). Every app the fleet runs, one click away:
// a header with the watch's own freshness, a one-line verdict, the launch
// search (type, Enter opens the first match in a new tab), then the apps as
// kp action tiles grouped by their stack, each with one state word (answers,
// failing, down, deploying, not watched, the word is the legend), a hover
// card that says why an app is amber or red, facets that keep only the apps
// in a state, and a menu per stack that opens every app of it that answers.
//
// Everything on a tile is declared in the stack that owns the route
// (`tiles:` in lxc-compose.yml); nothing here knows an app. The state is the
// minute watch (`/data/watch`): the tile's own probe, or its stack's
// container when the tile has none.

import { fetchJson, h } from "../dom.js";
import { formatDateTime } from "../format.js";
import { declare, drivable } from "../drivable.js";
import { agoEl, setAgo } from "../ago.js";
import { appsVerdict, filterTiles, markParts, tileState } from "../appsview.js";
import { ensureStyle, keyRow, pageHeader } from "../ui.js";
import { attachCardStates, setCardState } from "/static/kp/js/card-state.js";
import {
  FACET_EVENT,
  attachFacets,
  setFacetCounts,
  setFacetItems,
  setFacetValues,
} from "/static/kp/js/facets.js";
import { attachHoverCards } from "/static/kp/js/hovercard.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { attachMenuButtons } from "/static/kp/js/menu-button.js";

/**
 * @typedef {{stack: string, host: string, url: string, name: string,
 *   group: string, order: number, description?: string, lines: string[],
 *   error?: string}} Tile
 */

// Live view (invariant 39; coordinator rule 2026-10-03: every clickable
// element carries a declared, stable id): the tiles, the search, the Stacks
// link, the state facets, a stack's Open menu and the empty search's reset.
const OPEN = declare({
  id: "apps-open",
  page: "home",
  opens: "run",
  row: "<tile host>",
  what: "open one app in a new tab",
});
const SEARCH = declare({
  id: "apps-search",
  page: "home",
  opens: "view",
  what: "the launch search: type an app's name, Enter opens the first match",
});
const STACKS = declare({
  id: "apps-stacks",
  page: "home",
  opens: "view",
  what: "the header's Stacks: every stack with its state and actions",
});
const RETRY = declare({
  id: "apps-retry",
  page: "home",
  opens: "view",
  what: "read the apps again after a failed read",
  shows: "after the apps could not be read",
});
const SHOW_ALL = declare({
  id: "apps-show-all",
  page: "home",
  opens: "view",
  what: "clear the app search and show every app again",
  shows: "when the search matches no app",
});
const FACET = declare({
  id: "apps-state-filter",
  page: "home",
  opens: "view",
  row: "answers|failing|down|deploying|not watched",
  what: "keep only the apps in this state (turn the state on or off)",
});
const OPEN_MENU = declare({
  id: "apps-stack-menu",
  page: "home",
  opens: "view",
  row: "<stack>",
  what: "open the Open menu of one stack's apps",
});
const OPEN_ALL = declare({
  id: "apps-open-all",
  page: "home",
  opens: "run",
  row: "<stack>",
  what: "open every app of this stack that answers, each in its own tab",
  shows: "while a stack's Open menu is open",
  reach: [{ do: "click", control: "apps-stack-menu", row: "*" }],
});

/** How often the dots are read again (the watch runs every minute). */
const EVERY_MS = 60_000;

/** The words a tile's state can say; the longest sets the width. */
const WORDS = "answers\nfailing\ndown\ndeploying\nnot watched";
const STATE_ORDER = ["answers", "failing", "down", "deploying", "not watched"];

/**
 * The one word of a tile's state.
 * @param {ReturnType<typeof tileState>} st
 */
export const wordOf = (st) =>
  st.state === "up"
    ? "answers"
    : st.state === "flaky"
      ? "failing"
      : st.state === "unwatched"
        ? "not watched"
        : st.state;

/**
 * Tiles in page order, cut into their stacks.
 * @param {Tile[]} tiles already sorted by the host
 * @returns {{stack: string, tiles: Tile[]}[]}
 */
export function byStack(tiles) {
  /** @type {Map<string, Tile[]>} */
  const out = new Map();
  for (const t of tiles) out.set(t.stack, [...(out.get(t.stack) ?? []), t]);
  return [...out].map(([stack, list]) => ({ stack, tiles: list }));
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/stacks.css");
  root.classList.add("ap-page");
  const abort = new AbortController();
  /** @type {Tile[] | null} */
  let tiles = null;
  /** @type {Map<string, import("../appsview.js").Watch>} */
  let watch = new Map();
  /** @type {string | null} why the tiles could not be read */
  let failed = null;
  let q = new URLSearchParams(location.search).get("q") ?? "";
  /** @type {string[] | null} the states kept; null: all */
  let kept = null;

  const stacksLink = drivable(
    h(
      "a",
      {
        class: "kp-button",
        href: "/stacks",
        title: "Every stack with its state and actions",
      },
      "Stacks",
    ),
    STACKS,
  );
  const watched = h(
    "span",
    {
      class: "nx-live",
      title: "The dashboard asks every app once a minute whether it answers",
    },
    "watched every minute",
  );
  const head = pageHeader({
    title: "Apps",
    desc: "Every app the fleet runs, one click away. Type to find one, Enter opens it in a new tab; the word on a tile says whether it answers.",
    actions: [stacksLink],
  });
  head.title.after(watched);
  head.el.classList.add("sk-head");
  const verdictDot = h("span", { class: "ap-dot" });
  const verdictText = h("strong", null, "Reading the apps…");
  const verdictAgo = agoEl("checked");
  const verdict = h(
    "p",
    { class: "ap-verdict", role: "status" },
    verdictDot,
    verdictText,
    h("span", null, " · "),
    verdictAgo,
  );
  const search = /** @type {HTMLInputElement} */ (
    h("input", {
      type: "search",
      class: "kp-field__input nx-search ap-search",
      placeholder: matchMedia("(max-width: 30rem)").matches
        ? "Open an app…"
        : "Open an app… type its name, Enter opens it",
      "aria-label": "Find an app",
      "aria-keyshortcuts": "/",
    })
  );
  search.value = q;
  drivable(search, SEARCH);
  const launch = h(
    "div",
    { class: "ap-launch" },
    search,
    keyRow([
      [["/"], "find"],
      [["Enter"], "open"],
      [["←", "→", "↑", "↓"], "move"],
    ]),
  );

  // The launcher is a kp card with the four data states: its loading is the
  // package's busy card over the whole footprint, its empty and failed
  // plates fill the same box.
  const facets = h(
    "div",
    {
      class: "kp-facets kp-facets--row ap-facets",
      "data-kp-facets": "",
      "data-kp-facets-all": "Everything",
      role: "group",
      "aria-label": "State of the apps",
    },
    h("ul", { class: "kp-facets__list" }),
  );
  const body = h("div", { class: "ap-stacks" });
  const data = h("div", { class: "ap-launcher" }, facets, body);
  const card = h(
    "section",
    {
      class: "kp-card ap-card",
      "data-kp-state": "loading",
      "aria-label": "Apps by stack",
    },
    h(
      "div",
      { class: "kp-card__stage" },
      h("div", { class: "kp-card__data" }, data),
    ),
  );
  root.replaceChildren(head.el, verdict, launch, card);
  const detachFacets = attachFacets(root);
  const detachCards = attachCardStates(root);
  const detachMarks = attachIdmarks(root);
  setFacetItems(
    facets,
    STATE_ORDER.map((v) => ({ value: v, label: v })),
  );
  facets.querySelectorAll("input[type=checkbox]").forEach((box) => {
    if (box instanceof HTMLElement && box.getAttribute("value"))
      drivable(box, FACET, box.getAttribute("value") ?? "");
  });

  /** @param {string} text */
  const marked = (text) =>
    markParts(text, q).map((p) => (p.hit ? h("mark", null, p.text) : p.text));
  const mark = (/** @type {string} */ name) =>
    h("span", {
      class: "kp-idmark kp-idmark--sm",
      "data-kp-idmark": name,
      "aria-hidden": "true",
    });

  /** @param {Tile} t @param {boolean} first */
  const tileEl = (t, first) => {
    const st = tileState(watch, t);
    const a = h(
      "a",
      {
        class: `kp-action ap-tile${first ? " is-first" : ""}`,
        href: t.url,
        target: "_blank",
        rel: "noopener",
        "data-host": t.host,
        "data-kp-hovercard": "",
        "data-ap-state": wordOf(st),
      },
      h(
        "strong",
        { class: "kp-action__title" },
        mark(t.name),
        ...marked(t.name),
      ),
      h("span", { class: "kp-action__mark", "aria-hidden": "true" }),
      h(
        "span",
        { class: "kp-action__text" },
        h(
          "span",
          { class: "kp-state-word", "data-kp-words": WORDS },
          wordOf(st),
        ),
      ),
    );
    drivable(a, OPEN, t.host);
    return a;
  };

  /** What the hover card says about a tile. @param {HTMLElement} anchor */
  const fill = (anchor) => {
    const t = tiles?.find((x) => x.host === anchor.dataset.host);
    if (!t) return null;
    const st = tileState(watch, t);
    const w = watch.get(`tile:${t.host}`) ?? watch.get(`stack:${t.stack}`);
    const word = wordOf(st);
    const now =
      word === "failing" || word === "down"
        ? `${word}${w?.failing_since ? ` since ${formatDateTime(w.failing_since)}` : ""}${w?.why ? `, ${w.why}` : ""}`
        : word === "deploying"
          ? `deploying${w?.why ? `: ${w.why}` : ""}`
          : word === "not watched"
            ? "No probe is configured for this app."
            : `answers${st.checkedAt ? `, checked ${formatDateTime(st.checkedAt)}` : ""}`;
    const row = (/** @type {string} */ k, /** @type {string} */ v) =>
      h("div", { class: "kp-kv__row" }, h("dt", null, k), h("dd", null, v));
    return h(
      "div",
      { class: "ap-hover" },
      h("b", { class: "ap-hover__title" }, t.name),
      h(
        "dl",
        { class: "kp-kv" },
        row("Stack", t.stack),
        row("Address", t.host),
        row("Now", now),
        ...(st.via === "stack"
          ? [
              row(
                "Probe",
                `none of its own: the stack ${t.stack} is asked instead`,
              ),
            ]
          : []),
      ),
    );
  };
  const detachHover = attachHoverCards(root, { fill });

  const visible = () => {
    const base = tiles ? filterTiles(tiles, q) : [];
    return kept
      ? base.filter((t) => kept?.includes(wordOf(tileState(watch, t))))
      : base;
  };

  const paintBody = () => {
    if (!tiles) return;
    const shown = visible();
    const groups = byStack(shown);
    let first = q.trim() !== "";
    body.replaceChildren(
      ...groups.map((g) => {
        const ok = g.tiles.filter(
          (t) => wordOf(tileState(watch, t)) === "answers",
        );
        const menu = h(
          "div",
          {
            class: "kp-menu-button ap-group__menu",
            "data-kp-menu-button": "",
            "data-kp-key": `open-${g.stack}`,
          },
          drivable(
            h(
              "button",
              { type: "button", class: "kp-button kp-button--sm" },
              "Open ▾",
            ),
            OPEN_MENU,
            g.stack,
          ),
          h(
            "div",
            {
              class: "kp-menu kp-menu--rich",
              role: "menu",
              "aria-label": `Apps of ${g.stack}`,
              hidden: "",
            },
            drivable(
              h(
                "button",
                {
                  type: "button",
                  role: "menuitem",
                  class: "kp-menu__item",
                  tabindex: "-1",
                  "data-kp-value": g.stack,
                  ...(ok.length ? {} : { "aria-disabled": "true" }),
                },
                h(
                  "span",
                  { class: "kp-menu__label" },
                  `Open the ${ok.length} ${ok.length === 1 ? "app" : "apps"} that answer`,
                ),
                h(
                  "span",
                  { class: "kp-menu__hint" },
                  ok.length
                    ? "each in its own tab"
                    : "none of them answers now",
                ),
              ),
              OPEN_ALL,
              g.stack,
            ),
          ),
        );
        menu.addEventListener("kp-menu-select", () => {
          for (const t of ok) window.open(t.url, "_blank", "noopener");
        });
        return h(
          "div",
          { class: "ap-group", "data-stack": g.stack },
          h(
            "h3",
            { class: "ap-group__name" },
            mark(g.stack),
            g.stack,
            h("span", { class: "kp-badge" }, String(g.tiles.length)),
          ),
          menu,
          h(
            "div",
            {
              class: "kp-actions",
              role: "group",
              "aria-label": `Apps of ${g.stack}`,
            },
            ...g.tiles.map((t) => {
              const el = tileEl(t, first);
              first = false;
              return el;
            }),
          ),
        );
      }),
    );
    attachMenuButtons(body);
    if (!groups.length) {
      const all = h(
        "button",
        {
          type: "button",
          class: "kp-button",
          title: "Clear the search and the state filter (Esc)",
        },
        "Show every app",
      );
      drivable(all, SHOW_ALL);
      all.addEventListener("click", () => {
        kept = null;
        setFacetValuesAll();
        setQuery("");
      });
      setCardState(card, "empty", {
        title: q.trim()
          ? `No app matches “${q.trim()}”`
          : "No app is in this state",
        text: q.trim()
          ? "Search looks at an app's name, address, description and stack."
          : "Pick Everything to see all the apps again.",
        action: all,
      });
    } else setCardState(card, "ready");
  };

  const setFacetValuesAll = () => {
    import("/static/kp/js/facets.js").then((m) =>
      m.setFacetValues(facets, null),
    );
  };
  facets.addEventListener(FACET_EVENT, (e) => {
    const values = /** @type {CustomEvent} */ (e).detail.values;
    kept = values && values.length < STATE_ORDER.length ? values : null;
    paintBody();
  });
  card.addEventListener("kp-card-retry", () => void load());

  const paint = () => {
    if (failed != null) {
      verdictText.textContent = "The apps could not be read";
      verdictDot.className = "ap-dot ap-dot--bad";
      setCardState(card, "error", {
        title: "The apps could not be read",
        text: `${failed}. They come back by themselves once the host answers.`,
      });
      const retry = card.querySelector(
        ".kp-card__note button, .kp-stateplate button",
      );
      if (retry instanceof HTMLElement && !retry.dataset.drive)
        drivable(retry, RETRY);
      return;
    }
    if (tiles == null) {
      setCardState(card, "loading", { words: "Reading the apps" });
      return;
    }
    const v = appsVerdict(tiles, watch);
    verdictDot.className = `ap-dot${v.tone ? ` ap-dot--${v.tone}` : ""}`;
    verdictText.textContent = tiles.length ? v.text : "No apps yet";
    setAgo(verdictAgo, v.checkedAt);
    if (!tiles.length) {
      setCardState(card, "empty", {
        title: "No app has a tile yet",
        text: "A tile comes from the stack that owns the app's address: add tiles: to its lxc-compose.yml and deploy it.",
      });
      return;
    }
    const counts = Object.fromEntries(
      STATE_ORDER.map((w) => [
        w,
        tiles?.filter((t) => wordOf(tileState(watch, t)) === w).length ?? 0,
      ]),
    );
    setFacetCounts(facets, counts);
    paintBody();
  };

  /** @param {string} next */
  const setQuery = (next) => {
    q = next;
    if (search.value !== next) search.value = next;
    const p = new URLSearchParams(location.search);
    if (q) p.set("q", q);
    else p.delete("q");
    const s = p.toString();
    history.replaceState(
      history.state,
      "",
      `${location.pathname}${s ? `?${s}` : ""}`,
    );
    if (tiles) paintBody();
  };
  search.addEventListener("input", () => setQuery(search.value));
  search.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const f = visible()[0];
      if (f) window.open(f.url, "_blank", "noopener");
    } else if (e.key === "ArrowDown") {
      e.preventDefault();
      /** @type {HTMLElement | null} */ (
        body.querySelector(".ap-tile")
      )?.focus();
    } else if (e.key === "Escape" && search.value) {
      e.stopPropagation();
      setQuery("");
    }
  });
  // Arrows move between tiles on the grid they sit in.
  body.addEventListener("keydown", (e) => {
    const all = /** @type {HTMLElement[]} */ ([
      ...body.querySelectorAll(".ap-tile"),
    ]);
    const i = all.indexOf(/** @type {HTMLElement} */ (document.activeElement));
    if (i < 0) return;
    const grid = all[i].parentElement;
    const cols = grid
      ? Math.max(1, Math.round(grid.clientWidth / all[i].offsetWidth))
      : 1;
    /** @type {Record<string, number>} */
    const step = {
      ArrowRight: 1,
      ArrowLeft: -1,
      ArrowDown: cols,
      ArrowUp: -cols,
    };
    const d = step[e.key];
    if (d == null) return;
    e.preventDefault();
    if (e.key === "ArrowUp" && i - cols < 0) {
      search.focus();
      return;
    }
    all[Math.max(0, Math.min(all.length - 1, i + d))].focus();
  });

  const readWatch = async () => {
    const w = await fetchJson("/data/watch", "the watch", abort.signal);
    if (!w.ok) return;
    /** @type {import("../appsview.js").Watch[]} */
    const targets = w.body.targets ?? [];
    watch = new Map(targets.map((x) => [x.key, x]));
    paint();
  };
  const load = async () => {
    failed = null;
    tiles = null;
    paint();
    const [r] = await Promise.all([
      fetchJson("/data/tiles", "the apps", abort.signal),
      readWatch(),
    ]);
    if (abort.signal.aborted) return;
    if (!r.ok) {
      failed = `${r.error.what}: ${r.error.why}`;
      paint();
      return;
    }
    tiles = r.body.report?.tiles ?? [];
    paint();
  };
  paint();
  void load().catch(() => {});
  const timer = setInterval(() => void readWatch().catch(() => {}), EVERY_MS);
  if (!matchMedia("(max-width: 48rem)").matches) search.focus();
  return () => {
    abort.abort();
    clearInterval(timer);
    detachFacets();
    detachCards();
    detachMarks();
    detachHover();
    root.classList.remove("ap-page");
  };
}
