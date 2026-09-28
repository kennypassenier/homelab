// The dashboard shell (arch-frontend): the navigation bar, the history-API
// router and the one store. Each page is its own module with a `mount`
// that returns its cleanup; chassis answers index.html for every
// extensionless path under /app/, so a deep link lands here too
// (feat-overview-8). The palette, the shortcuts, the theme menu and the
// host's questions sit around every page (chrome.js).

import { startAgoTicker } from "./ago.js";
import { mountChrome } from "./chrome.js";
import { h } from "./dom.js";
import { navEntries, pageTitle, route } from "./router.js";
import { current, start, subscribe } from "./store.js";
import { mount as activity } from "./pages/activity.js";
import { mount as checks } from "./pages/checks.js";
import { mount as doctor } from "./pages/doctor.js";
import { mount as firewall } from "./pages/firewall.js";
import { mount as host } from "./pages/host.js";
import { mount as jobs } from "./pages/jobs.js";
import { mount as notifications } from "./pages/notifications.js";
import { mount as schedules } from "./pages/schedules.js";
import { mount as settings } from "./pages/settings.js";
import { mount as overview } from "./pages/overview.js";
import { mount as stack } from "./pages/stack.js";
import { mount as timeline } from "./pages/timeline.js";

const page = /** @type {HTMLElement} */ (document.getElementById("page"));
const nav = /** @type {HTMLElement} */ (document.getElementById("nav"));
const bar = /** @type {HTMLElement} */ (document.getElementById("bar"));
const asks = /** @type {HTMLElement} */ (document.getElementById("asks"));
const link = /** @type {HTMLElement} */ (document.getElementById("link"));

/** @type {() => void} */
let cleanup = () => {};

/** @param {string} href a path under /app/, with its query string */
function navigate(href) {
  if (href === location.pathname + location.search) return;
  history.pushState(null, "", href);
  render();
}

function render() {
  cleanup();
  cleanup = () => {};
  const r = route(location.pathname);
  document.title = pageTitle(r);
  nav.replaceChildren(
    ...navEntries(r).map((n) => {
      /** @type {Record<string, string>} */
      const a = { class: "kp-nav__link", href: n.href };
      if (n.current) a["aria-current"] = "page";
      return h("li", null, h("a", a, n.label));
    }),
  );
  switch (r.page) {
    case "overview":
      cleanup = overview(page, { navigate });
      break;
    case "host":
      cleanup = host(page, { navigate });
      break;
    case "stack":
      cleanup = stack(page, { name: r.name, tab: r.tab, navigate });
      break;
    case "activity":
      cleanup = activity(page);
      break;
    case "timeline":
      cleanup = timeline(page, { navigate });
      break;
    case "checks":
      cleanup = checks(page);
      break;
    case "doctor":
      cleanup = doctor(page);
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
    case "settings":
      cleanup = settings(page);
      break;
    default:
      page.replaceChildren(
        h("h1", null, "Not found"),
        h(
          "p",
          null,
          `There is no page at ${r.path}. `,
          h("a", { href: "/app/" }, "Go to the overview"),
        ),
      );
  }
  window.scrollTo(0, 0);
}

// Same-origin links under /app/ stay in the page; a modified click (new
// tab, download) is the browser's.
document.addEventListener("click", (e) => {
  if (e.defaultPrevented || e.button !== 0) return;
  if (e.metaKey || e.ctrlKey || e.shiftKey || e.altKey) return;
  const a = /** @type {Element} */ (e.target).closest("a");
  if (!a || a.target || a.hasAttribute("download")) return;
  const url = new URL(a.href, location.href);
  if (url.origin !== location.origin || !url.pathname.startsWith("/app/"))
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
startAgoTicker();
start();
render();
