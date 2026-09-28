// The dashboard shell (arch-frontend): the navigation bar, the history-API
// router and the one store. Each page is its own module with a `mount`
// that returns its cleanup; chassis answers index.html for every
// extensionless path under /app/, so a deep link lands here too.

import { h } from "./dom.js";
import { navEntries, pageTitle, route } from "./router.js";
import { current, start, subscribe } from "./store.js";
import { mount as activity } from "./pages/activity.js";
import { mount as checks } from "./pages/checks.js";
import { mount as doctor } from "./pages/doctor.js";
import { mount as overview } from "./pages/overview.js";
import { mount as stack } from "./pages/stack.js";

const page = /** @type {HTMLElement} */ (document.getElementById("page"));
const nav = /** @type {HTMLElement} */ (document.getElementById("nav"));
const link = /** @type {HTMLElement} */ (document.getElementById("link"));

/** @type {() => void} */
let cleanup = () => {};

/** @param {string} href */
function navigate(href) {
  if (href === location.pathname) return;
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
    case "stack":
      cleanup = stack(page, { name: r.name });
      break;
    case "activity":
      cleanup = activity(page);
      break;
    case "checks":
      cleanup = checks(page);
      break;
    case "doctor":
      cleanup = doctor(page);
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
  navigate(url.pathname);
});
window.addEventListener("popstate", render);

subscribe(() => {
  const s = current();
  link.textContent = s.link;
  link.dataset.up = String(s.up);
});
start();
render();
