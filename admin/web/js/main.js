// The dashboard shell (arch-frontend): the navigation bar, the history-API
// router and the one store. Each page is its own module with a `mount`
// that returns its cleanup; chassis answers index.html for every
// extensionless path this app's web app (mounted at the root since
// chassis-rs 3.1.0) does not already claim, so a deep link lands here too
// (feat-overview-8). The palette, the shortcuts, the theme menu and the
// running pill sit around every page (chrome.js). The nav bar renders from
// the page registry (`GET /api/kit/pages`, pages.js).
//
// feat-shell-1 (redesign 3.71.0, Kenny approved 2026-10-03): six areas,
// `Apps · Inbox · Stacks · Activity │ Backups · System` (areas.js), the
// Inbox's exact count on its link and in the tab title, the trail above
// every page below an area's own (`Stacks / gateway / Logs`), and `/` that
// opens the Inbox when it holds something and Apps otherwise. Every old
// address redirects (router.js `redirectFor`). A page that a 3.71.0 page
// does not draw yet is mounted as a view of its new home (`VIEWS`:
// `/activity?view=planned` shows the Schedules module) until that page's
// own redesign lands.

import { startAgoTicker } from "./ago.js";
import { areaOf, crumbs } from "./areas.js";
import { mountChrome } from "./chrome.js";
import { mountConnectionBar } from "./connectionbar.js";
import { mountSystemRail } from "./systemrail.js";
import { startFollowingSystem } from "./thisbrowser.js";
import { mountFollow } from "./drive.js";
import { h } from "./dom.js";
import { countText, inboxNow, onInbox, worst } from "./inboxlist.js";
import {
  attachNavMenus,
  attachNavToggles,
  attachNavFolds,
  attachMotion,
  attachTableRegions,
  attachActionColumns,
  attachTileSets,
  attachKpiStrips,
  attachKpiToggles,
  attachDismissals,
  attachSelects,
  attachLoadingPieces,
  attachFootprints,
  attachCardStates,
  setCardState,
} from "/static/kp/dist/kp-themes.min.js";
import { loadPages, pages, subscribePages } from "./pages.js";
import {
  STACK_TABS,
  VIEWS,
  navEntries,
  needsFleet,
  pageTitle,
  redirectFor,
  route,
} from "./router.js";
import { current, start, subscribe } from "./store.js";
import { stage } from "./stage.js";
import { breadcrumbs } from "./ui.js";
import { mount as activity } from "./pages/activity.js";
import { mount as backups } from "./pages/backups.js";
import { mount as firewall } from "./pages/firewall.js";
import { mount as host } from "./pages/host.js";
import { mount as notifications } from "./pages/notifications.js";
import { mount as restorePage } from "./pages/restore.js";
import { mount as retired } from "./pages/retired.js";
import { mount as settings } from "./pages/settings.js";
import { mount as overview } from "./pages/overview.js";
import { mount as stack } from "./pages/stack.js";
import { mount as shell } from "./pages/shell.js";
import { mount as presets } from "./pages/presets.js";
import { mount as storagePage } from "./pages/storage.js";
import { mount as homePage } from "./pages/home.js";
import { mount as inboxPage } from "./pages/needsyou.js";
import { mount as updatePage } from "./pages/update.js";
import { scopeOf } from "./updateflow.js";
import { mount as systemPage } from "./pages/system.js";
import { mount as metricsPage } from "./pages/metrics.js";
import { mount as fleetviewPage } from "./pages/fleetview.js";
import { mount as passkeysPage } from "./pages/passkeys.js";
import { mountVersions } from "./versions.js";
import { installTableResets } from "./tableresets.js";

installTableResets();
// "Follow my system" (Settings ▸ This browser) works from every page.
startFollowingSystem();

const page = /** @type {HTMLElement} */ (document.getElementById("page"));
const nav = /** @type {HTMLElement} */ (document.getElementById("nav"));
const bar = /** @type {HTMLElement} */ (document.getElementById("bar"));
const brand = /** @type {HTMLElement} */ (document.getElementById("brand"));
const trail = /** @type {HTMLElement} */ (document.getElementById("crumbs"));

const rail = mountSystemRail(
  /** @type {HTMLElement} */ (document.getElementById("rail-slot")),
);

/** @type {() => void} */
let cleanup = () => {};

/** @param {string} href an absolute path, with its query string */
function navigate(href) {
  if (href === location.pathname + location.search) return;
  history.pushState(null, "", href);
  closeBar();
  render();
}

/**
 * A page change inside the app leaves the bar as a page load would: the
 * phone menu closed and no dropdown held open by the focus in it.
 */
function closeBar() {
  if (bar.hasAttribute("data-kp-nav-open"))
    /** @type {HTMLElement | null} */ (
      bar.querySelector("[data-kp-nav-toggle]")
    )?.click();
  const f = document.activeElement;
  if (f instanceof HTMLElement && bar.contains(f)) f.blur();
}

/** The last bar drawn, as markup: a refresh that builds the same one skips the redraw. */
let navSignature = "";

/**
 * The nav bar, the brand link, the trail and the tab title — everything
 * the page registry (pages.js) and the Inbox drive — redrawn on their own,
 * so the registry's first answer (async, after the first paint) and a new
 * Inbox row show up without a page remount.
 */
function renderNav() {
  const r = route(location.pathname);
  const ps = pages();
  const { items } = inboxNow();
  const n = items.length;
  // FLOWS.md §1.2: the count is in the tab title too, so a background tab
  // says it ("● 3 in the Inbox · Homelab · Stacks").
  document.title = `${n > 0 ? `● ${n} in the Inbox · ` : ""}${pageTitle(r, ps)}`;
  // chassis-rs 3.2.0 (fix-15): App::brand_title("Homelab") makes
  // ps.brand.title the display title. The static text stays as the
  // first-paint value until the registry answers.
  if (ps) {
    brand.textContent = ps.brand.title;
    brand.setAttribute("href", ps.brand.href);
  }
  const entries = navEntries(ps, r, areaOf);
  const tone = worst(items);
  const built = entries.flatMap((e, i) => {
    /** @type {Record<string, string>} */
    const a = { class: "kp-nav__link", href: e.href };
    if (e.current) a["aria-current"] = "page";
    const p = ps?.pages.find((x) => x.path === e.href);
    const out = [];
    // The divider before the rarely needed areas (FLOWS.md §1).
    if (p && p.id === "backups" && i > 0)
      out.push(h("li", { class: "nx-nav-sep", "aria-hidden": "true" }));
    const kids = [/** @type {Node | string} */ (e.label)];
    if (p?.id === "needs-you" && n > 0) {
      kids.push(
        h(
          "span",
          {
            class: `kp-badge nx-count${tone === "bad" ? " kp-badge--destructive" : " kp-badge--warning"}`,
            "aria-label": `${n} waiting`,
            "data-inbox-count": String(n),
          },
          countText(n),
        ),
      );
    }
    if (p) a["data-area"] = p.id;
    out.push(
      h(
        "li",
        p?.id === "needs-you" ? { "data-kp-nav-keep": "" } : null,
        h("a", a, ...kids),
      ),
    );
    return out;
  });
  // A refresh that changes nothing leaves the bar alone: the first-visit tour
  // marks a link in it (`data-kp-tour-target`), and a bar drawn again would
  // take the mark with the old link. When the bar does change, the mark moves
  // to the new link of the same area.
  const signature = built
    .map((x) => /** @type {Element} */ (x).outerHTML)
    .join("");
  if (signature !== navSignature) {
    navSignature = signature;
    const marked = nav
      .querySelector("[data-kp-tour-target]")
      ?.getAttribute("data-area");
    nav.replaceChildren(...built);
    if (marked)
      nav
        .querySelector(`a[data-area="${marked}"]`)
        ?.setAttribute("data-kp-tour-target", "");
  }
  const list = crumbs(r, location.pathname, location.search, {
    tabLabel: (t) => STACK_TABS.find((x) => x.tab === t)?.label ?? t,
  });
  trail.replaceChildren(...(list.length > 1 ? [breadcrumbs(list)] : []));
  trail.hidden = list.length < 2;
  rail.sync(areaOf(r) === "system", location.pathname, location.search);
  chrome.repaint();
}

/**
 * A page that a 3.71.0 page has not redrawn yet, shown as a view of its
 * new home (router.js `VIEWS`): the module mounted for `?view=` /
 * `?section=`, or the page's own.
 * @param {string} at the route's page
 * @param {URLSearchParams} q
 * @returns {string | null} the module's page id, or null for the page's own
 */
function viewOf(at, q) {
  for (const [id, v] of Object.entries(VIEWS))
    if (v.at === at && q.get(v.param) === v.value) return id;
  return null;
}

/** @type {Record<string, (root: HTMLElement) => () => void>} */
const VIEW_MOUNTS = {
  // redesign-activity: Activity draws these as its own views.
  log: (root) => activity(root, { navigate }),
  schedules: (root) => activity(root, { navigate }),
  retired: (root) => retired(root),
  passkeys: (root) => passkeysPage(root),
  // redesign-final-h5: the Doctor's old address opens Host on its checks.
  doctor: (root) => host(root, { navigate, focus: "checks" }),
};

/**
 * The page while it waits for the first read: kp's own busy card over a
 * page-sized data-state card (the package's loading state, never plain words).
 * @param {string} words what is being read
 * @returns {() => void} take the card's wiring away
 */
function bootCard(words) {
  const card = stage(document.createElement("div"));
  card.classList.add("boot-stage");
  setCardState(card, "loading", { words });
  const holder = h("div", { class: "boot" }, card);
  page.replaceChildren(holder);
  return attachCardStates(holder);
}

/**
 * `/`: the Inbox when it holds something, Apps otherwise (Kenny,
 * 2026-10-03). Waits, with the busy card, until the Inbox's live sources have
 * answered once (at most 3 s), then sends the address on.
 */
function landing() {
  const unboot = bootCard("Opening the dashboard");
  const decide = () => {
    const { items, ready } = inboxNow();
    if (!ready && Date.now() - since < 3000) return false;
    if (location.pathname !== "/") return true;
    history.replaceState(
      null,
      "",
      `${items.length > 0 ? "/inbox" : "/apps"}${location.search}`,
    );
    render();
    return true;
  };
  const since = Date.now();
  if (decide()) return unboot;
  const off = onInbox(() => {
    if (decide()) off();
  });
  const t = setTimeout(() => {
    if (decide()) off();
  }, 3100);
  return () => {
    off();
    clearTimeout(t);
    unboot();
  };
}

function render() {
  // Every retired address (router.js REDIRECTS) goes on to its new home
  // before anything mounts; /secrets without ?stack= first waits for the
  // fleet, to know its first stack.
  const r0 = route(location.pathname);
  if (needsFleet(r0, location.search) && !current().fleet) {
    cleanup();
    const unboot = bootCard("Finding the stack");
    const off = subscribe(() => {
      if (!current().fleet) return;
      off();
      render();
    });
    cleanup = () => {
      off();
      unboot();
    };
    return;
  }
  const target = redirectFor(r0, location.search, {
    stacks: (current().fleet?.stacks ?? []).map((s) => s.name),
  });
  if (target != null) {
    history.replaceState(null, "", target);
    render();
    return;
  }
  // planned-tab-hang-1 (Kenny, 2026-10-26: leaving Activity's Now view hangs
  // the tab): kp's detach of a table puts every row back in the markup, and on
  // a page that is on screen each step makes the browser lay it out again
  // (1.8 s for the 175 operations of the demo, 2 s from Now to Planned). The
  // page is about to be replaced, so what it drew leaves the document first
  // and every part is detached there, where nothing is laid out.
  const leaving = document.createDocumentFragment();
  leaving.append(...page.childNodes);
  cleanup();
  cleanup = () => {};
  leaving.replaceChildren();
  const r = route(location.pathname);
  renderNav();
  const q = new URLSearchParams(location.search);
  const view = viewOf(r.page, q);
  if (view && VIEW_MOUNTS[view]) {
    cleanup = VIEW_MOUNTS[view](page);
    window.scrollTo(0, 0);
    return;
  }
  switch (r.page) {
    case "landing":
      cleanup = landing();
      break;
    case "home":
      cleanup = homePage(page);
      break;
    case "needs-you":
      cleanup = inboxPage(page);
      break;
    case "restore":
      cleanup = restorePage(page, { navigate });
      break;
    case "update":
      cleanup = updatePage(page, scopeOf(location.search) ?? { all: true });
      break;
    case "overview":
      cleanup = overview(page, { navigate });
      break;
    case "system":
      cleanup = systemPage(page);
      break;
    case "metrics":
      cleanup = metricsPage(page, { navigate });
      break;
    case "host":
      cleanup = host(page, { navigate });
      break;
    case "stack":
      cleanup = stack(page, { name: r.name, tab: r.tab, navigate });
      break;
    case "activity":
      cleanup = activity(page, { navigate });
      break;
    case "notifications":
      cleanup = notifications(page);
      break;
    case "firewall":
      cleanup = firewall(page);
      break;
    case "backups":
      cleanup = backups(page);
      break;
    case "settings":
      cleanup = settings(page);
      break;
    case "shell":
      cleanup = shell(page);
      break;
    case "presets":
      cleanup = presets(page, { navigate });
      break;
    case "storage":
      cleanup = storagePage(page);
      break;
    case "fleetview":
      cleanup = fleetviewPage(page);
      break;
    default:
      page.replaceChildren(
        h("h1", null, "Not found"),
        h(
          "p",
          null,
          `There is no page at ${location.pathname}. `,
          h("a", { href: "/" }, "Go to the start"),
        ),
      );
  }
  window.scrollTo(0, 0);
}

// Paths the kit answers itself, never this app's router.
const RESERVED = [
  "/api",
  "/static",
  "/login",
  "/logout",
  "/healthz",
  "/readyz",
  "/metrics",
];

// Same-origin links this app's router can name stay in the page; a
// modified click (new tab, download) and a reserved or file path are the
// browser's.
document.addEventListener("click", (e) => {
  if (e.defaultPrevented || e.button !== 0) return;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const a = /** @type {Element} */ (e.target).closest("a");
  if (!a || a.target || a.hasAttribute("download")) return;
  const url = new URL(a.href, location.href);
  if (url.origin !== location.origin) return;
  if (
    RESERVED.some((p) => url.pathname === p || url.pathname.startsWith(`${p}/`))
  )
    return;
  if (/\.[a-z0-9]+$/i.test(url.pathname)) return;
  e.preventDefault();
  // A link to a section of the page already shown scrolls to it; it does
  // not draw the page again.
  if (
    url.hash &&
    url.pathname === location.pathname &&
    url.search === location.search
  ) {
    history.replaceState(null, "", url.hash);
    document
      .getElementById(decodeURIComponent(url.hash.slice(1)))
      ?.scrollIntoView({ behavior: "smooth", block: "start" });
    return;
  }
  navigate(url.pathname + url.search + url.hash);
});
window.addEventListener("popstate", render);

mountConnectionBar(
  /** @type {HTMLElement} */ (document.getElementById("connection")),
);
const chrome = mountChrome(
  { nav: bar },
  { navigate, route: () => route(location.pathname) },
);
// feat-platform-10: the "Live view" switch and badge, on every page.
// redesign-final X7: in the bar beside Help, not a row of its own above
// every page's breadcrumb (no demo has that row; two lines on a phone).
const follow = h("div", {
  class: "follow follow--bar",
  id: "follow",
  role: "group",
  "aria-label": "Claude driving this dashboard",
});
// fix-371-1 (Kenny, 2026-10-04: at 390 px the theme button wrapped to a row
// of its own): Live view, Help and the theme button are one group at the
// bar's end, so when a narrow bar has to wrap (a running job's pill beside
// a wide brand face) they move together, never the theme button alone.
const barEnd = h("div", { class: "nx-bar-end" }, follow);
for (const el of [
  bar.querySelector(":scope > .help-button"),
  bar.querySelector(":scope > .theme-slot"),
])
  if (el) barEnd.append(el);
bar.append(barEnd);
mountFollow(follow, { navigate });
// TUI parity: a newer host release, a dashboard older than its host.
mountVersions(/** @type {HTMLElement} */ (document.getElementById("versions")));
startAgoTicker();
start();
render();
// The registry answers after the first paint; redraw the nav (never the
// mounted page) the moment it lands, and again whenever the Inbox moves.
subscribePages(renderNav);
onInbox(renderNav);
void loadPages();
// kp-themes' bar: the phone menu button, and dropdowns kept inside the window.
attachNavToggles(bar);
attachNavMenus(bar);

// feat-kp11demo-14: the bar is one row in every theme (Kenny, 2026-09-29),
// and what does not fit folds from the end behind kp's More menu; the Inbox
// (`data-kp-nav-keep`, renderNav) never folds, because it carries the count
// that needs a person. Before kp-themes 11 `fitBar` folded ALL the links
// behind the phone toggle as soon as one wrapped.
attachNavFolds(bar);
// kp-themes' motion on the whole dashboard (Kenny, 2026-10-08: "alles wat
// kp-themes te bieden heeft ten volle benutten"): every card eases to its
// new height, and what is added to one arrives the theme's way. A live
// repaint is not news: under `arrive: "new"` a row put back under the key
// it left with (`data-kp-row-key`, `id`), or a like-for-like redraw, stays
// still; what replaces a skeleton is the data it waited for.
attachMotion(document, {
  size: ".kp-card, .nx-card, [data-motion-box]",
  arrive: "new",
});
// kp-themes' tables.js: every table's scroll box a named region a keyboard
// can reach (tabindex, role, the caption as its name), for the tables the
// pages draw now and later.
attachTableRegions(document);
// kp-themes' actions.js: a row's buttons of one role share one column and
// one width down the whole table (`.kp-row-actions`).
attachActionColumns(document);
// kp-themes' tiles.js: the tiles of every grid on one board (the Apps
// groups, `data-kp-tiles-set`) share one height.
attachTileSets(document);
// kp-themes' kpi.js: a key figure strip takes the column count that fits its
// own width, and a tile that toggles a filter flips and says so (ui.js kpi).
attachKpiStrips(document);
attachKpiToggles(document);
// kp-themes' overlays.js: an alert's ✕ closes it the theme's way (explainNote).
attachDismissals(document);
// kp-themes' loading-pieces.js: a theme that draws its loading pictures from
// pieces (pastel's caterpillar) gets them on every part that waits; the other
// themes draw nothing extra.
attachLoadingPieces(document);
attachFootprints(document);
// kp-themes' combobox.js: every single select is the theme's drawn list over
// the native select, which keeps the value (Live view and `.value` code are
// unaffected); `data-kp-select="native"` opts one out. A page or a dialog
// drawn later is covered by the observer.
attachSelects(document);
new MutationObserver((records) => {
  const roots = new Set();
  for (const r of records)
    if ([...r.addedNodes].some((n) => n instanceof Element))
      roots.add(r.target);
  for (const root of roots)
    if (root instanceof Element && root.isConnected) {
      attachTableRegions(root);
      // kp-themes' footprint.js: a data component keeps the height it had
      // while its loading, empty or failed state shows (every page, every
      // component drawn later, held once).
      attachFootprints(root);
    }
}).observe(page, { childList: true, subtree: true });
new MutationObserver((records) => {
  for (const r of records)
    for (const n of r.addedNodes)
      if (n instanceof Element && n.isConnected) attachSelects(n);
}).observe(document.body, { childList: true, subtree: true });
