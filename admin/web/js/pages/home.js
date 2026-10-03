// The Apps page (redesign-stacks, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/apps.html). Home when
// the Inbox is empty (`/` sends there; main.js `landing`). Every app the
// fleet runs, one click away: a header with the watch's own freshness, a
// one-line verdict, the launch search (type, Enter opens the first match
// in a new tab), then the tiles grouped as the stacks declare them, the
// starred ones on top, and a legend of the dots.
//
// Everything on a tile is declared in the stack that owns the route
// (`tiles:` in lxc-compose.yml); nothing here knows an app. The dot is the
// minute watch (`/data/watch`): the tile's own probe, or its stack's
// container when the tile has none. Stars are this browser's own (a
// per-viewer convenience, never shared state).

import { fetchJson, h } from "../dom.js";
import { formatDateTime } from "../format.js";
import { declare, drivable } from "../drivable.js";
import { agoEl, setAgo } from "../ago.js";
import {
  appsVerdict,
  board,
  filterTiles,
  hueOf,
  initials,
  markParts,
  readStars,
  tileState,
} from "../appsview.js";
import { emptyState, pageHeader } from "../ui.js";
import { art, ensureStyle, keyRow } from "./stackskit.js";

/**
 * @typedef {{stack: string, host: string, url: string, name: string,
 *   group: string, order: number, description?: string, lines: string[],
 *   error?: string}} Tile
 */

/**
 * Tiles in page order, cut into their groups.
 * @param {Tile[]} tiles already sorted by the host
 * @returns {{group: string, tiles: Tile[]}[]}
 */
export function grouped(tiles) {
  /** @type {{group: string, tiles: Tile[]}[]} */
  const out = [];
  for (const t of tiles) {
    const last = out[out.length - 1];
    if (last && last.group === t.group) last.tiles.push(t);
    else out.push({ group: t.group, tiles: [t] });
  }
  return out;
}

// Live view (invariant 39; coordinator rule 2026-10-03: every clickable
// element carries a declared, stable id): the tiles, their stars, the
// search, the Stacks link and the empty search's reset.
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
});
const STAR = declare({
  id: "apps-star",
  page: "home",
  opens: "view",
  row: "<tile host>",
  what: "star or unstar one app, keeping it at the top under Starred",
});
const SHOW_ALL = declare({
  id: "apps-show-all",
  page: "home",
  opens: "view",
  what: "clear the app search and show every app again",
});

/** Where this browser keeps its starred tiles. */
const STARS_KEY = "homelab.apps.starred";

/** How often the dots are read again (the watch runs every minute). */
const EVERY_MS = 60_000;

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
  /** @type {Set<string>} */
  let stars = new Set();
  try {
    stars = readStars(localStorage.getItem(STARS_KEY));
  } catch {
    // a private window: no stars kept, every tile in its own group
  }
  const saveStars = () => {
    try {
      localStorage.setItem(STARS_KEY, JSON.stringify([...stars]));
    } catch {
      // the stars last as long as this page
    }
  };

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
    desc: "Every app the fleet runs, one click away. Type to find one, Enter opens it in a new tab; star the ones you use most.",
    actions: [stacksLink],
  });
  // As the demo: the live status beside the title, the link on the right.
  head.title.after(watched);
  head.el.classList.add("sk-head");
  const verdictDot = h("span", { class: "sk-dot" });
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
      [["s"], "star"],
    ]),
  );
  const body = h("div", { class: "ap-board", "aria-label": "Apps" });
  const legend = h(
    "p",
    { class: "ap-legend", "aria-label": "What the dots mean" },
    ...[
      ["ok", "answers"],
      ["warn", "failing, not yet down"],
      ["bad", "down"],
      ["info", "deploying"],
      ["", "not watched"],
    ].map(([tone, words]) =>
      h("span", { class: `sk-dot${tone ? ` sk-dot--${tone}` : ""}` }, words),
    ),
  );
  root.replaceChildren(head.el, verdict, launch, body, legend);

  /** @param {string} text */
  const marked = (text) =>
    markParts(text, q).map((p) => (p.hit ? h("mark", null, p.text) : p.text));

  /** @param {Tile} t */
  const star = (t) => {
    if (stars.has(t.host)) stars.delete(t.host);
    else stars.add(t.host);
    saveStars();
    paint();
    /** @type {HTMLElement | null} */ (
      body.querySelector(`.ap-tile[data-host="${CSS.escape(t.host)}"]`)
    )?.focus();
  };

  /**
   * @param {Tile} t
   * @param {boolean} first
   */
  const tileEl = (t, first) => {
    const st = tileState(watch, t);
    const pinned = stars.has(t.host);
    const pin = drivable(
      h(
        "button",
        {
          type: "button",
          class: "ap-tile__pin",
          "aria-pressed": String(pinned),
          "aria-label": pinned ? `Unstar ${t.name}` : `Star ${t.name}`,
          title: pinned
            ? "Remove from Starred (s)"
            : "Keep at the top under Starred (s)",
        },
        pinned ? "★" : "☆",
      ),
      STAR,
      t.host,
    );
    pin.addEventListener("click", (e) => {
      e.preventDefault();
      e.stopPropagation();
      star(t);
    });
    const why =
      st.via === "stack"
        ? `its stack ${t.stack} ${st.state === "up" ? "runs" : `is ${st.word}`} (the tile has no probe of its own)`
        : st.word;
    const a = h(
      "a",
      {
        class: `ap-tile${first ? " is-first" : ""}`,
        href: t.url,
        target: "_blank",
        rel: "noopener",
        "data-host": t.host,
        title: `Open ${t.name} in a new tab\n${t.host} · ${why}${st.checkedAt ? ` · checked ${formatDateTime(st.checkedAt)}` : ""}\nstack ${t.stack}`,
      },
      h(
        "span",
        { class: "ap-tile__icon", "aria-hidden": "true" },
        initials(t.name),
      ),
      h(
        "span",
        { class: "ap-tile__name" },
        h("span", {
          class: `sk-dot${st.tone ? ` sk-dot--${st.tone}` : ""}`,
          role: "img",
          "aria-label": st.word,
        }),
        h("span", null, ...marked(t.name)),
      ),
      pin,
      h("span", { class: "ap-tile__desc" }, ...marked(t.description || t.host)),
      h("span", { class: "ap-tile__go", "aria-hidden": "true" }, "↗"),
      ...(t.lines.length || t.error
        ? [
            h(
              "span",
              { class: "ap-tile__lines" },
              t.error ? "reading failed" : t.lines.join(" · "),
            ),
          ]
        : []),
    );
    a.style.setProperty("--c", `var(--chart-${hueOf(t.name)})`);
    drivable(a, OPEN, t.host);
    a.addEventListener("keydown", (e) => {
      if (e.key === "s" && !e.ctrlKey && !e.metaKey && !e.altKey) {
        e.preventDefault();
        star(t);
      }
    });
    return a;
  };

  // Loading: the grouped board the page fills in (22rem columns, a label
  // per group), so nothing moves when the tiles arrive (rule 6).
  const paintLoading = () => {
    body.replaceChildren(
      ...[3, 2, 2].map((n, i) =>
        h(
          "section",
          {
            class: "ap-grp ap-grp--skeleton",
            "aria-label": "Reading the apps",
            role: "status",
            "data-kp-state": "loading",
            ...(i === 0 ? {} : { "aria-hidden": "true" }),
          },
          h("h2", null, h("span", { class: "kp-skeleton" })),
          h(
            "div",
            { class: "ap-tiles" },
            ...Array.from({ length: n }, () =>
              h(
                "div",
                { class: "ap-tile ap-tile--skeleton" },
                h("span", { class: "kp-skeleton" }),
                h("span", { class: "kp-skeleton" }),
                h("span", { class: "kp-skeleton" }),
              ),
            ),
          ),
        ),
      ),
    );
  };

  const paint = () => {
    if (failed != null) {
      body.replaceChildren(
        h(
          "div",
          {
            class: "kp-alert kp-alert--destructive ap-grp--wide",
            role: "alert",
          },
          h("strong", null, "The apps could not be read"),
          h(
            "span",
            null,
            ` ${failed}. They come back by themselves once the host answers; or `,
          ),
          drivable(h("a", { href: "/apps" }, "try again now"), RETRY),
          ".",
        ),
      );
      return;
    }
    if (tiles == null) {
      paintLoading();
      return;
    }
    const v = appsVerdict(tiles, watch);
    verdictDot.className = `sk-dot${v.tone ? ` sk-dot--${v.tone}` : ""}`;
    verdictText.textContent = tiles.length ? v.text : "No apps yet";
    setAgo(verdictAgo, v.checkedAt);
    if (!tiles.length) {
      body.replaceChildren(
        h(
          "section",
          { class: "kp-card nx-card ap-grp--wide ap-empty" },
          emptyState({
            art: art.noTiles(),
            title: "No app has a tile yet",
            text: "A tile comes from the stack that owns the app's address: add tiles: to its lxc-compose.yml and deploy it.",
            action: h(
              "pre",
              { class: "ap-example sk-mono" },
              "tiles:\n  jellyfin.example.dev:\n    name: Jellyfin\n    group: Media\n    description: Films and series",
            ),
          }),
        ),
      );
      return;
    }
    const groups = board(tiles, stars, q);
    let first = q.trim() !== "";
    const shown = filterTiles(tiles, q);
    const sections = groups.map((g) =>
      h(
        "section",
        {
          class: `ap-grp${g.wide ? " ap-grp--wide" : ""}`,
          "aria-label": g.group,
        },
        h(
          "h2",
          null,
          g.group,
          h("span", { class: "sk-chip" }, String(g.tiles.length)),
        ),
        h(
          "div",
          { class: "ap-tiles" },
          ...g.tiles.map((t) => {
            const el = tileEl(t, first);
            first = false;
            return el;
          }),
        ),
      ),
    );
    if (!shown.length) {
      const all = drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button",
            title: "Clear the search (Esc)",
          },
          "Show every app",
        ),
        SHOW_ALL,
      );
      all.addEventListener("click", () => setQuery(""));
      sections.push(
        h(
          "div",
          { class: "ap-grp--wide sk-empty-slot" },
          emptyState({
            art: art.noMatch(),
            title: `No app matches “${q.trim()}”`,
            text: "Search looks at an app's name, address, description and stack.",
            action: all,
          }),
        ),
      );
    }
    body.replaceChildren(...sections);
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
    paint();
  };
  search.addEventListener("input", () => setQuery(search.value));
  search.addEventListener("keydown", (e) => {
    if (e.key === "Enter") {
      const f = tiles ? filterTiles(tiles, q)[0] : undefined;
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
      ...body.querySelectorAll(".ap-tile:not(.ap-tile--skeleton)"),
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
  paint();
  (async () => {
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
  })().catch(() => {});
  const timer = setInterval(() => void readWatch().catch(() => {}), EVERY_MS);
  if (!matchMedia("(max-width: 48rem)").matches) search.focus();
  return () => {
    abort.abort();
    clearInterval(timer);
    root.classList.remove("ap-page");
  };
}
