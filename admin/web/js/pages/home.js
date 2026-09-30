// The home page (replace-homepage, Kenny 2026-09-30, renamed from Start):
// one tile per service, grouped, each opening its app in a new tab, with
// the line its stack file's reading prints. Everything on a tile is
// declared in the stack that owns the route (`tiles:` in lxc-compose.yml);
// nothing here knows an app.
//
// Below the tiles: a health strip (decision replace-homepage-2), shown only
// when something needs Kenny — Today's own items (doctor, the fleet check
// and the manual checks, whichever this tab has already read: `keptRead`,
// never a fresh 90 s read just to paint a strip) and a tile or container
// `/data/watch` reports down. Collapsed it is one line; it expands in place
// to the list and links on to the Health page.

import { h, fetchJson } from "../dom.js";
import { formatTime } from "../format.js";
import { todayView } from "../parity.js";
import { keptRead } from "../slowread.js";

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

/**
 * @typedef {{key: string, failing_since: number | null, down: boolean,
 *   state: "up" | "flaky" | "down" | "deploying", why: string | null}} Watch
 */

/**
 * The dot beside a tile's name: what the minute watch (replace-kuma) says.
 * Decision "deploys are known outages" (Kenny, 2026-09-30): a stack known
 * to be deploying gets its own dot, never the down (red) one — it is an
 * expected outage, not a fault.
 * @param {Watch | undefined} w
 */
function dot(w) {
  if (!w) return h("span", { class: "start-dot", title: "not watched yet" });
  if (w.state === "deploying")
    return h("span", {
      class: "start-dot start-dot--deploying",
      title: "deploying",
    });
  if (w.down)
    return h("span", {
      class: "start-dot start-dot--down",
      title: `no answer since ${formatTime(w.failing_since ?? 0)}: ${w.why ?? ""}`,
    });
  if (w.failing_since != null)
    return h("span", {
      class: "start-dot start-dot--flaky",
      title: `failing since ${formatTime(w.failing_since)}: ${w.why ?? ""}`,
    });
  return h("span", { class: "start-dot start-dot--up", title: "answers" });
}

/**
 * @param {Tile} t
 * @param {Map<string, Watch>} watch
 */
function tileEl(t, watch) {
  /** @type {(string | Node)[]} */
  const parts = [
    h(
      "span",
      { class: "start-tile__name" },
      dot(watch.get(`tile:${t.host}`)),
      t.name,
    ),
  ];
  if (t.description)
    parts.push(h("span", { class: "start-tile__desc" }, t.description));
  for (const l of t.lines)
    parts.push(h("span", { class: "start-tile__line" }, l));
  if (t.error)
    parts.push(
      h(
        "span",
        { class: "start-tile__error", title: t.error },
        "reading failed",
      ),
    );
  // Kenny, 2026-09-30: a tile opens the app in its own tab — the dashboard
  // stays open behind it.
  return h(
    "a",
    { class: "start-tile", href: t.url, target: "_blank", rel: "noopener" },
    ...parts,
  );
}

/**
 * One health-strip problem: what it is, and when it started when known.
 * @typedef {{text: string, since: number | null}} Problem
 */

/**
 * The down watch targets, as problems (the tiles already on screen show
 * their own dot; this is what makes them worth a strip).
 * @param {Watch[]} targets
 * @returns {Problem[]}
 */
function watchProblems(targets) {
  return targets
    .filter((w) => w.down)
    .map((w) => ({
      text: `${w.key.replace(/^tile:/, "")} does not answer${w.why ? `: ${w.why}` : ""}`,
      since: w.failing_since,
    }));
}

/**
 * The health strip: collapsed, one line with the count and the oldest
 * problem; expanded, every problem, each linking on to Health.
 * @param {Problem[]} problems
 */
function healthStrip(problems) {
  if (!problems.length) return null;
  const sorted = [...problems].sort(
    (a, b) => (a.since ?? Infinity) - (b.since ?? Infinity),
  );
  const first = sorted[0];
  const glyph = h("span", { class: "health-strip__glyph" }, "▸");
  const summary = h(
    "button",
    { type: "button", class: "health-strip__toggle", "aria-expanded": "false" },
    glyph,
    ` ${problems.length} problem${problems.length === 1 ? "" : "s"}: `,
    first.since != null ? `${formatTime(first.since)} ` : "",
    first.text,
  );
  const list = h(
    "ul",
    { class: "health-strip__list", hidden: "" },
    ...sorted.map((p) =>
      h("li", null, p.since != null ? `${formatTime(p.since)} ` : "", p.text),
    ),
    h("li", null, h("a", { href: "/app/health" }, "Open Health")),
  );
  summary.addEventListener("click", () => {
    const open = summary.getAttribute("aria-expanded") === "true";
    summary.setAttribute("aria-expanded", String(!open));
    glyph.textContent = open ? "▸" : "▾";
    list.hidden = open;
  });
  return h("div", { class: "health-strip", role: "status" }, summary, list);
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const status = h(
    "p",
    { class: "measured", role: "status" },
    "Reading the tiles…",
  );
  const body = h("div", { class: "start" });
  const strip = h("div", { class: "health-strip-slot" });
  root.replaceChildren(h("h1", null, "Home"), status, body, strip);
  const abort = new AbortController();
  (async () => {
    const [r, w] = await Promise.all([
      fetchJson("/data/tiles", "the start page", abort.signal),
      fetchJson("/data/watch", "the watch", abort.signal),
    ]);
    /** @type {Watch[]} */
    const targets = w.ok ? (w.body.targets ?? []) : [];
    /** @type {Map<string, Watch>} */
    const watch = new Map(targets.map((x) => [x.key, x]));
    if (!r.ok) {
      status.textContent = `${r.error.what}: ${r.error.why}`;
      return;
    }
    /** @type {Tile[]} */
    const tiles = r.body.report?.tiles ?? [];
    status.textContent = tiles.length
      ? ""
      : "No stack declares a tile yet: add `tiles:` to a stack file and deploy it.";
    body.replaceChildren(
      ...grouped(tiles).map((g) =>
        h(
          "section",
          { class: "start-group" },
          h("h2", null, g.group),
          h(
            "div",
            { class: "start-grid" },
            ...g.tiles.map((t) => tileEl(t, watch)),
          ),
        ),
      ),
    );
    // The strip is built from what this tab already knows: Today's own
    // items (a kept reading, never a fresh 90 s one just for the strip)
    // and the watch this same load already read.
    const kept = keptRead("/data/today");
    const todayProblems = kept
      ? todayView(kept).items.map((i) => ({
          text: `${i.source}: ${i.what}`,
          since: null,
        }))
      : [];
    const problems = [...todayProblems, ...watchProblems(targets)];
    const el = healthStrip(problems);
    strip.replaceChildren(...(el ? [el] : []));
  })().catch(() => {});
  return () => abort.abort();
}
