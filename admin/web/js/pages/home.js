// The Apps page (redesign-stacks, release 3.71.0; rebuilt on the approved
// kp-themes 11 demo, feat-kp11demo-12.4). Home when the Inbox is empty (`/`
// sends there; main.js `landing`). Every app the fleet runs, one click away:
// a header with the watch's own freshness, a one-line verdict, the launch
// search (type, Enter opens the first match in a new tab), then the apps as
// kp entity tiles (kind "tile": the whole card is the app's link, with a
// favourite star) grouped by their stack, each with one state word (answers,
// failing, down, deploying, not watched, the word is the legend), a hover
// card that says why an app is amber or red, facets that keep only the apps
// in a state, a menu per stack that opens every app of it that answers, the
// kp refresh control for the minute watch, and the arrow keys / first letters
// over the tiles.
//
// Everything on a tile is declared in the stack that owns the route
// (`tiles:` in lxc-compose.yml); nothing here knows an app. The state is the
// minute watch (`/data/watch`): the tile's own probe, or its stack's
// container when the tile has none.

import { shareApps } from "../appslist.js";
import { fetchJson, h } from "../dom.js";
import { formatDateTime } from "../format.js";
import { declare, drivable } from "../drivable.js";
import { agoEl, setAgo } from "../ago.js";
import { appsVerdict, filterTiles, tileState } from "../appsview.js";
import { ensureStyle, keyRow, pageHeader } from "../ui.js";
import {
  REFRESH_EVENT,
  attachRefresh,
  refreshOf,
  attachCardStates,
  setCardState,
  CONNECTION_RETRY_EVENT,
  attachConnection,
  setConnection,
  ENTITY_GRID_RETRY_EVENT,
  ENTITY_RETRY_EVENT,
  ENTITY_STAR_EVENT,
  attachEntityCards,
  setEntityGrid,
  attachTileKeys,
  tileStop,
  FACET_EVENT,
  attachFacets,
  setFacetCounts,
  setFacetItems,
  setFacetValues,
  attachHoverCards,
  attachUptimes,
  setUptime,
  attachMenuButtons,
} from "/static/kp/dist/kp-themes.min.js";
import { periodsOf, shareText, tileKey, uptimeQuery } from "../uptimeview.js";

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

/** How often the dots are read again by default (the watch runs every minute). */
const EVERY_S = 60;

/** Where the favourite apps are kept (this browser only). */
const STARS_KEY = "homelab.apps.starred";

/** The kp tone of each state word. */
const TONE = /** @type {const} */ ({
  answers: "success",
  failing: "warning",
  down: "destructive",
  deploying: "info",
  "not watched": undefined,
});

/** @returns {Set<string>} the hosts the person starred */
function readStars() {
  try {
    const v = JSON.parse(localStorage.getItem(STARS_KEY) ?? "[]");
    return new Set(Array.isArray(v) ? v.map(String) : []);
  } catch {
    return new Set();
  }
}
/** @param {Set<string>} set */
function writeStars(set) {
  try {
    localStorage.setItem(STARS_KEY, JSON.stringify([...set]));
  } catch {
    /* private mode: the stars last for this visit */
  }
}
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
  /**
   * The 90-day history (`/data/uptime`): null while it is read, a string
   * when it could not be, else the server's keys.
   * @type {Map<string, {periods?: any[], uptime?: number | null}> | string | null}
   */
  let uptimeHist = null;
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
  const verdictAgo = agoEl("checked", null, { live: true });
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
        ? "Open an app"
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
  // kp-themes 11.2.0's tile keys span every stack's grid below, in reading
  // order: arrows, Home, End and Page keys by place on screen, first letters
  // of an app's name, and Up from the first row to the search box.
  const body = h("div", {
    class: "ap-stacks",
    "data-kp-idmark-set": "apps",
    "data-kp-tiles-keys": "letters",
    "data-kp-tiles-keys-exit": ".ap-search",
  });
  const refreshEl = h("div", {
    class: "kp-refresh ap-refresh",
    "data-kp-refresh": String(EVERY_S),
    "data-kp-look": "segmented",
    "data-kp-refresh-choices": "0,15,30,60,300",
    "data-kp-remember": "apps-refresh",
    "data-kp-stale-after": String(EVERY_S * 3),
  });
  const bar = h("div", { class: "ap-bar ap-bar--wide" }, facets, refreshEl);
  const data = h("div", { class: "ap-launcher" }, bar, body);
  const connection = h("div", {
    class: "kp-connection",
    "data-kp-connection": "",
    "data-kp-placement": "top",
    "data-kp-connection-source": "app",
  });
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
  root.replaceChildren(head.el, connection, verdict, launch, card);
  const detachFacets = attachFacets(root);
  const detachCards = attachCardStates(root);
  const detachEntities = attachEntityCards(root);
  const detachTileKeys = attachTileKeys(body);
  const detachRefresh = attachRefresh(root);
  const detachConnection = attachConnection(root);
  setFacetItems(
    facets,
    STATE_ORDER.map((v) => ({ value: v, label: v })),
  );
  facets.querySelectorAll("input[type=checkbox]").forEach((box) => {
    if (box instanceof HTMLElement && box.getAttribute("value"))
      drivable(box, FACET, box.getAttribute("value") ?? "");
  });

  /** The favourites, kept in this browser. */
  const stars = readStars();

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
    // ask-kp11be-2: the 90 days this app answered, one cell a day.
    const strip = h("div", {
      class: "kp-uptime ap-hover__uptime",
      "data-kp-uptime": "",
      "data-kp-uptime-count": "90",
    });
    const entry =
      uptimeHist instanceof Map
        ? uptimeHist.get(tileKey(uptimeHist, t))
        : undefined;
    setUptime(strip, {
      label: "Answered, last 90 days",
      state:
        uptimeHist === null
          ? "loading"
          : typeof uptimeHist === "string"
            ? "error"
            : entry?.periods?.some((p) => p.state !== "none")
              ? "ready"
              : "empty",
      periods: periodsOf(entry, "day"),
      from: "90 days ago",
      to: "Today",
      count: 90,
      open: "info",
      ...(typeof uptimeHist === "string" ? { text: uptimeHist } : {}),
      message: "No reading yet: the watch has not measured this app.",
    });
    queueMicrotask(() => attachUptimes(strip));
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
        row("Last 90 days", shareText(entry?.uptime, "90 days")),
      ),
      strip,
    );
  };
  const detachHover = attachHoverCards(root, { fill });

  const visible = () => {
    const base = tiles ? filterTiles(tiles, q) : [];
    return kept
      ? base.filter((t) => kept?.includes(wordOf(tileState(watch, t))))
      : base;
  };

  /** @param {Tile} t @returns {import("/static/kp/dist/kp-themes.min.js").EntityData} */
  const entityOf = (t) => {
    const word = wordOf(tileState(watch, t));
    return {
      key: t.host,
      name: t.name,
      // The typed part of the name is marked (kp 11.2.0 `match`).
      ...(q.trim() ? { match: q.trim() } : {}),
      href: t.url,
      external: true,
      starred: stars.has(t.host),
      description: t.description ?? "",
      state: {
        label: word,
        tone: TONE[/** @type {keyof typeof TONE} */ (word)],
      },
      lines: [t.host, ...(t.lines ?? [])].slice(0, 3),
      ...(t.error
        ? { status: /** @type {const} */ ("error"), error: t.error }
        : {}),
    };
  };

  /**
   * One group per stack, made once and refilled: a card under the pointer or
   * the focus is never rebuilt by the minute watch.
   * @type {Map<string, {el: HTMLElement, badge: HTMLElement, label: HTMLElement,
   *   hint: HTMLElement, item: HTMLElement, grid: HTMLElement, ok: Tile[]}>}
   */
  const groupsBy = new Map();
  /** @param {string} stack */
  const groupOf = (stack) => {
    const have = groupsBy.get(stack);
    if (have) return have;
    const badge = h("span", { class: "kp-badge" });
    const label = h("span", { class: "kp-menu__label" });
    const hint = h("span", { class: "kp-menu__hint" });
    const item = h(
      "button",
      {
        type: "button",
        role: "menuitem",
        class: "kp-menu__item",
        tabindex: "-1",
        "data-kp-value": stack,
      },
      label,
      hint,
    );
    drivable(item, OPEN_ALL, stack);
    const menu = h(
      "div",
      {
        class: "kp-menu-button ap-group__menu",
        "data-kp-menu-button": "",
        "data-kp-key": `open-${stack}`,
      },
      drivable(
        h(
          "button",
          { type: "button", class: "kp-button kp-button--sm" },
          "Open ▾",
        ),
        OPEN_MENU,
        stack,
      ),
      h(
        "div",
        {
          class: "kp-menu kp-menu--rich",
          role: "menu",
          "aria-label": `Apps of ${stack}`,
          hidden: "",
        },
        item,
      ),
    );
    const grid = h("div", {
      class: "kp-entity-grid ap-tiles",
      "data-kp-entity-grid": "",
      "data-kp-entity-kind": "tile",
      "data-kp-entity-min": "14rem",
      "data-kp-entity-star-key": ".",
      role: "group",
      "aria-label": `Apps of ${stack}`,
    });
    const el = h(
      "div",
      { class: "ap-group", "data-stack": stack },
      h("h3", { class: "ap-group__name" }, stack, badge),
      menu,
      grid,
    );
    const g = {
      el,
      badge,
      label,
      hint,
      item,
      grid,
      ok: /** @type {Tile[]} */ ([]),
    };
    menu.addEventListener("kp-menu-select", () => {
      for (const t of g.ok) window.open(t.url, "_blank", "noopener");
    });
    groupsBy.set(stack, g);
    attachMenuButtons(el);
    return g;
  };

  /** Every tile card on the board, in reading order. */
  const cardsOf = () =>
    /** @type {HTMLElement[]} */ ([
      ...body.querySelectorAll("[data-kp-entity]"),
    ]);
  /** The star is reached with its key, not with Tab: the tile is the stop. */
  const quietStars = () => {
    for (const star of body.querySelectorAll(".kp-entity__star"))
      star.setAttribute("tabindex", "-1");
  };

  const paintBody = () => {
    if (!tiles) return;
    const shown = visible();
    const groups = byStack(shown);
    for (const g of groups) {
      const view = groupOf(g.stack);
      const ok = g.tiles.filter(
        (t) => wordOf(tileState(watch, t)) === "answers",
      );
      view.ok = ok;
      view.badge.textContent = String(g.tiles.length);
      view.label.textContent = `Open the ${ok.length} ${ok.length === 1 ? "app" : "apps"} that answer`;
      view.hint.textContent = ok.length
        ? "each in its own tab"
        : "none of them answers now";
      if (ok.length) view.item.removeAttribute("aria-disabled");
      else view.item.setAttribute("aria-disabled", "true");
      const ordered = [...g.tiles].sort(
        (a, b) => Number(stars.has(b.host)) - Number(stars.has(a.host)),
      );
      setEntityGrid(view.grid, ordered.map(entityOf));
      for (const c of view.grid.querySelectorAll("[data-kp-entity]")) {
        if (!(c instanceof HTMLElement)) continue;
        const t = g.tiles.find((x) => x.host === c.dataset.kpEntityKey);
        if (!t) continue;
        c.dataset.host = t.host;
        c.setAttribute("data-kp-hovercard", "");
        // On a phone the first tap on a tile opens its card, the second follows the link.
        c.setAttribute("data-kp-touch", "tap");
        const link = c.querySelector("a.kp-entity__name");
        if (link instanceof HTMLElement) drivable(link, OPEN, t.host);
      }
    }
    body.replaceChildren(
      ...groups.map((g) => /** @type {HTMLElement} */ (groupOf(g.stack).el)),
    );
    const live = new Set(groups.map((g) => g.stack));
    for (const k of groupsBy.keys()) if (!live.has(k)) groupsBy.delete(k);
    // The first match is the one Enter opens.
    cardsOf().forEach((c, i) =>
      c.toggleAttribute("data-ap-first", q.trim() !== "" && i === 0),
    );
    quietStars();
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
        setFacetValues(facets, null);
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

  facets.addEventListener(FACET_EVENT, (e) => {
    const values = /** @type {CustomEvent} */ (e).detail.values;
    kept = values && values.length < STATE_ORDER.length ? values : null;
    paintBody();
  });
  card.addEventListener("kp-card-retry", () => void load());
  body.addEventListener(ENTITY_RETRY_EVENT, () => void load());
  body.addEventListener(ENTITY_GRID_RETRY_EVENT, () => void load());
  body.addEventListener(ENTITY_STAR_EVENT, (e) => {
    const { key, starred } = /** @type {CustomEvent} */ (e).detail;
    if (starred) stars.add(key);
    else stars.delete(key);
    writeStars(stars);
    paintBody();
  });

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
      const first = cardsOf()[0];
      if (first) tileStop(first)?.focus();
    } else if (e.key === "Escape" && search.value) {
      e.stopPropagation();
      setQuery("");
    }
  });
  /** The capped 90-day history behind the hover card's strip. */
  const readHistory = async () => {
    const r = await fetchJson(
      `/data/uptime?${uptimeQuery("day", Math.floor(Date.now() / 1000))}`,
      "the uptime history",
      abort.signal,
    );
    if (abort.signal.aborted) return;
    uptimeHist = r.ok
      ? new Map(Object.entries(r.body.keys ?? {}))
      : `${r.error.what}: ${r.error.why}`;
  };

  /** @returns {Promise<boolean>} whether the watch could be read */
  const readWatch = async () => {
    const w = await fetchJson("/data/watch", "the watch", abort.signal);
    if (abort.signal.aborted) return true;
    if (!w.ok) {
      if (tiles)
        setConnection(connection, "refused", {
          why: `${w.error.what}: ${w.error.why}`,
        });
      return false;
    }
    setConnection(connection, "online");
    /** @type {import("../appsview.js").Watch[]} */
    const targets = w.body.targets ?? [];
    watch = new Map(targets.map((x) => [x.key, x]));
    paint();
    void readHistory().catch(() => {});
    return true;
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
    shareApps(r.body.report?.tiles ?? []);
    paint();
    refreshOf(refreshEl)?.now();
  };
  // The kp refresh control reads the watch at the chosen interval (and when
  // "Refresh now" is pressed); the control starts with the page's own load.
  refreshEl.addEventListener(REFRESH_EVENT, (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    void readWatch()
      .then((ok) => d.done(ok))
      .catch(() => d.done(false));
  });
  connection.addEventListener(CONNECTION_RETRY_EVENT, () => void readWatch());
  paint();
  void load().catch(() => {});
  if (!matchMedia("(max-width: 48rem)").matches) search.focus();
  return () => {
    abort.abort();
    detachFacets();
    detachCards();
    detachEntities();
    detachTileKeys();
    detachRefresh();
    detachConnection();
    detachHover();
    root.classList.remove("ap-page");
  };
}
