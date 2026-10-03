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
import { mountFollow } from "./drive.js";
import { attachRowToggle } from "./rowtoggle.js";
import { h } from "./dom.js";
import { countText, inboxNow, onInbox, worst } from "./inbox.js";
import { attachNavMenus, attachNavToggles } from "/static/kp/js/components.js";
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
import { breadcrumbs, skeletonLines } from "./ui.js";
import { mount as activity } from "./pages/activity.js";
import { mount as backups } from "./pages/backups.js";
import { mount as firewall } from "./pages/firewall.js";
import { mount as host } from "./pages/host.js";
import { mount as notifications } from "./pages/notifications.js";
import { mount as retired } from "./pages/retired.js";
import { mount as settings } from "./pages/settings.js";
import { mount as overview } from "./pages/overview.js";
import { mount as stack } from "./pages/stack.js";
import { mount as shell } from "./pages/shell.js";
import { mount as presets } from "./pages/presets.js";
import { mount as homePage } from "./pages/home.js";
import { mount as inboxPage } from "./pages/inbox.js";
import { mount as systemPage } from "./pages/system.js";
import { mount as metricsPage } from "./pages/metrics.js";
import { mount as fleetviewPage } from "./pages/fleetview.js";
import { mount as backupCalendarPage } from "./pages/backupcalendar.js";
import { mount as passkeysPage } from "./pages/passkeys.js";
import { mount as doctorPage } from "./pages/doctor.js";
import { mountVersions } from "./versions.js";

const page = /** @type {HTMLElement} */ (document.getElementById("page"));
const nav = /** @type {HTMLElement} */ (document.getElementById("nav"));
const bar = /** @type {HTMLElement} */ (document.getElementById("bar"));
const brand = /** @type {HTMLElement} */ (document.getElementById("brand"));
const trail = /** @type {HTMLElement} */ (document.getElementById("crumbs"));
const link = /** @type {HTMLElement} */ (document.getElementById("link"));

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
  nav.replaceChildren(
    ...entries.flatMap((e, i) => {
      /** @type {Record<string, string>} */
      const a = { class: "kp-nav__link", href: e.href };
      if (e.current) a["aria-current"] = "page";
      const p = ps?.pages.find((x) => x.path === e.href);
      const out = [];
      // The divider before the rarely needed areas (FLOWS.md §1).
      if (p && p.id === "backups" && i > 0)
        out.push(h("li", { class: "nx-nav-sep", "aria-hidden": "true" }));
      const kids = [/** @type {Node | string} */ (e.label)];
      if (p?.id === "inbox" && n > 0) {
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
      out.push(h("li", null, h("a", a, ...kids)));
      return out;
    }),
  );
  const list = crumbs(r, location.pathname, location.search, {
    tabLabel: (t) => STACK_TABS.find((x) => x.tab === t)?.label ?? t,
  });
  trail.replaceChildren(...(list.length > 1 ? [breadcrumbs(list)] : []));
  trail.hidden = list.length < 2;
  chrome.repaint();
  queueMicrotask(fitBar);
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
  backupcalendar: (root) => backupCalendarPage(root),
  retired: (root) => retired(root),
  passkeys: (root) => passkeysPage(root),
  doctor: (root) => doctorPage(root),
};

/**
 * `/`: the Inbox when it holds something, Apps otherwise (Kenny,
 * 2026-10-03). Waits, with a skeleton, until the Inbox's live sources have
 * answered once (at most 3 s), then sends the address on.
 */
function landing() {
  page.replaceChildren(skeletonLines(4, "Opening the dashboard"));
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
  if (decide()) return () => {};
  const off = onInbox(() => {
    if (decide()) off();
  });
  const t = setTimeout(() => {
    if (decide()) off();
  }, 3100);
  return () => {
    off();
    clearTimeout(t);
  };
}

function render() {
  // Every retired address (router.js REDIRECTS) goes on to its new home
  // before anything mounts; /secrets without ?stack= first waits for the
  // fleet, to know its first stack.
  const r0 = route(location.pathname);
  if (needsFleet(r0, location.search) && !current().fleet) {
    cleanup();
    page.replaceChildren(skeletonLines(3, "Finding the stack"));
    const off = subscribe(() => {
      if (!current().fleet) return;
      off();
      render();
    });
    cleanup = () => off();
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
  cleanup();
  cleanup = () => {};
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
    case "inbox":
      cleanup = inboxPage(page);
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
  navigate(url.pathname + url.search + url.hash);
});
window.addEventListener("popstate", render);

subscribe(() => {
  const s = current();
  link.textContent = s.link;
  link.dataset.up = String(s.up);
});
const chrome = mountChrome(
  { nav: bar },
  { navigate, route: () => route(location.pathname) },
);
// feat-platform-10: the "Live view" switch and badge, on every page.
mountFollow(/** @type {HTMLElement} */ (document.getElementById("follow")), {
  navigate,
});
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

/**
 * The bar in one row in every theme (Kenny, 2026-09-29): a theme whose font
 * is wide would wrap the links onto a second row, so the bar then folds its
 * links behind kp's menu button, as on a phone.
 */
function fitBar() {
  bar.removeAttribute("data-fold");
  const kids = [...bar.children].filter(
    (e) => e instanceof HTMLElement && e.offsetParent !== null,
  );
  if (kids.length === 0) return;
  const firstBottom = Math.min(
    ...kids.map((e) => e.getBoundingClientRect().bottom),
  );
  const wrapped = kids.some(
    (e) => e.getBoundingClientRect().top >= firstBottom,
  );
  if (wrapped) bar.setAttribute("data-fold", "");
}
fitBar();
addEventListener("resize", fitBar);
attachRowToggle();
// fix-176: the bar's content changes after the first paint, so it is
// measured again whenever its content or its own size moves. Only
// childList/characterData are watched: fitBar's own data-fold toggle is an
// attribute change and cannot re-trigger it.
new MutationObserver(fitBar).observe(bar, {
  childList: true,
  subtree: true,
  characterData: true,
});
if (typeof ResizeObserver === "function")
  new ResizeObserver(fitBar).observe(bar);
new MutationObserver(fitBar).observe(document.documentElement, {
  attributes: true,
  attributeFilter: ["data-theme", "class"],
});
void document.fonts?.ready.then(fitBar);
