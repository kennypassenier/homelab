// The dashboard shell (arch-frontend): the navigation bar, the history-API
// router and the one store. Each page is its own module with a `mount`
// that returns its cleanup; chassis answers index.html for every
// extensionless path this app's web app (mounted at the root since
// chassis-rs 3.1.0) does not already claim, so a deep link lands here too
// (feat-overview-8). The palette, the shortcuts, the theme menu and the
// host's questions sit around every page (chrome.js). The nav bar renders
// from the page registry (`GET /api/kit/pages`, pages.js), which is also
// how the kit's own pages (Status, Clients, Passkeys — `kit_pages_in_webapp`)
// end up in it without this module naming them.

import { startAgoTicker } from "./ago.js";
import { mountChrome } from "./chrome.js";
import { mountFollow } from "./drive.js";
import { h } from "./dom.js";
import { attachNavMenus, attachNavToggles } from "/static/kp/js/components.js";
import { loadPages, pages, subscribePages } from "./pages.js";
import { navEntries, pageTitle, redirectFor, route } from "./router.js";
import { current, start, subscribe } from "./store.js";
import { mount as activity } from "./pages/activity.js";
import { mount as backups } from "./pages/backups.js";
import { mount as firewall } from "./pages/firewall.js";
import { mount as host } from "./pages/host.js";
import { mount as jobs } from "./pages/jobs.js";
import { mount as notifications } from "./pages/notifications.js";
import { mount as schedules } from "./pages/schedules.js";
import { mount as secrets } from "./pages/secrets.js";
import { mount as settings } from "./pages/settings.js";
import { mount as overview } from "./pages/overview.js";
import { mount as stack } from "./pages/stack.js";
import { mount as hostLog } from "./pages/log.js";
import { mount as shell } from "./pages/shell.js";
import { mount as apply } from "./pages/apply.js";
import { mount as presets } from "./pages/presets.js";
import { mount as homePage } from "./pages/home.js";
import { mount as healthPage } from "./pages/health.js";
import { mount as metricsPage } from "./pages/metrics.js";
import { mount as fleetviewPage } from "./pages/fleetview.js";
import { mount as backupCalendarPage } from "./pages/backupcalendar.js";
import { mount as statusPage } from "./pages/status.js";
import { mount as clientsPage } from "./pages/clients.js";
import { mount as passkeysPage } from "./pages/passkeys.js";
import { mountVersions } from "./versions.js";

const page = /** @type {HTMLElement} */ (document.getElementById("page"));
const nav = /** @type {HTMLElement} */ (document.getElementById("nav"));
const bar = /** @type {HTMLElement} */ (document.getElementById("bar"));
const brand = /** @type {HTMLElement} */ (document.getElementById("brand"));
const asks = /** @type {HTMLElement} */ (document.getElementById("asks"));
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
 * The nav bar, the brand link and the tab title — everything the page
 * registry (pages.js) drives — redrawn on its own, so the registry's
 * first answer (arriving async, after the first paint) does not have to
 * wait for a page remount to show up.
 */
function renderNav() {
  const r = route(location.pathname);
  const ps = pages();
  document.title = pageTitle(r, ps);
  // fix-176: ps.brand.title is the registry's `app` field, which chassis-rs
  // fills from AppSpec.name ("homelab-admin" — the binary name, env prefix
  // and state-dir stem, not a display title; chassis has no separate brand
  // title to ask for). pageTitle() above already knows this and hardcodes
  // "Homelab" for the tab title; the bar's brand link keeps the same
  // hardcoded text (set once in index.html) instead of being overwritten
  // with the internal app name on every registry answer. Only the link
  // target (where "Apps" vs "Overview" sends it) comes from the registry.
  if (ps) {
    brand.setAttribute("href", ps.brand.href);
  }
  nav.replaceChildren(
    ...navEntries(ps, r).map((n) => {
      /** @type {Record<string, string>} */
      const a = { class: "kp-nav__link", href: n.href };
      if (n.current) a["aria-current"] = "page";
      if (!n.items) return h("li", null, h("a", a, n.label));
      a["aria-haspopup"] = "true";
      return h(
        "li",
        null,
        h("a", a, n.label),
        h(
          "ul",
          { class: "kp-nav__menu" },
          ...n.items.map((i) => {
            /** @type {Record<string, string>} */
            const ia = { href: i.href };
            if (i.current) ia["aria-current"] = "page";
            return h("li", null, h("a", ia, i.label));
          }),
        ),
      );
    }),
  );
  queueMicrotask(fitBar);
}

function render() {
  // 2026-09-30: /start, /today, /doctor, /checks, /charts, /traffic and
  // /timeline are retired, and /home is the pre-3.1.0 address of the tile
  // page (now the root); send every one of them on to its new home before
  // mounting anything (redirectFor is null for every other route).
  const target = redirectFor(route(location.pathname), location.search);
  if (target != null) {
    history.replaceState(null, "", target);
    render();
    return;
  }
  cleanup();
  cleanup = () => {};
  const r = route(location.pathname);
  renderNav();
  switch (r.page) {
    case "overview":
      cleanup = overview(page, { navigate });
      break;
    case "home":
      cleanup = homePage(page);
      break;
    case "health":
      cleanup = healthPage(page);
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
    case "jobs":
      cleanup = jobs(page, { navigate });
      break;
    case "schedules":
      cleanup = schedules(page);
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
    case "secrets":
      cleanup = secrets(page);
      break;
    case "settings":
      cleanup = settings(page);
      break;
    case "log":
      cleanup = hostLog(page);
      break;
    case "shell":
      cleanup = shell(page);
      break;
    case "apply":
      cleanup = apply(page);
      break;
    case "presets":
      cleanup = presets(page, { navigate });
      break;
    case "fleetview":
      cleanup = fleetviewPage(page);
      break;
    case "backupcalendar":
      cleanup = backupCalendarPage(page);
      break;
    // The kit's own pages (chassis-rs 3.1.0, `kit_pages_in_webapp`): this
    // app draws them from GET /api/kit/status|clients|passkeys, in this
    // same bar, instead of the kit's own layout.
    case "status":
      cleanup = statusPage(page);
      break;
    case "clients":
      cleanup = clientsPage(page);
      break;
    case "passkeys":
      cleanup = passkeysPage(page);
      break;
    default:
      page.replaceChildren(
        h("h1", null, "Not found"),
        h(
          "p",
          null,
          `There is no page at ${location.pathname}. `,
          h("a", { href: "/" }, "Go to Apps"),
        ),
      );
  }
  window.scrollTo(0, 0);
}

// Paths the kit answers itself, never this app's router (`/passkeys` is
// its registration/login actions; `/status` and `/clients` without
// `kit_pages_in_webapp` would be too, but the dashboard turns that on, so
// their GET pages fall through to this app like any other route).
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
  navigate(url.pathname + url.search);
});
window.addEventListener("popstate", render);

subscribe(() => {
  const s = current();
  link.textContent = s.link;
  link.dataset.up = String(s.up);
});
mountChrome(
  { nav: bar, asks },
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
// mounted page) the moment it lands, and again if it ever changes.
subscribePages(renderNav);
void loadPages();
// kp-themes' bar: the phone menu button, and dropdowns kept inside the window.
attachNavToggles(bar);
attachNavMenus(bar);

/**
 * The bar in one row in every theme (Kenny, 2026-09-29): a theme whose font
 * is wide (cyberpunk at 1280 px) would wrap the links onto a second row, so
 * the bar then folds its links behind kp's menu button, as on a phone.
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
new MutationObserver(fitBar).observe(document.documentElement, {
  attributes: true,
  attributeFilter: ["data-theme", "class"],
});
void document.fonts?.ready.then(fitBar);
