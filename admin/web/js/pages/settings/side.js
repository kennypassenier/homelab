// The side list with scroll-spy (kp-themes 11.2.0's "On this page"): this
// dashboard's cards, then host.toml's groups.

import { drivable } from "../../drivable.js";
import { h } from "../../dom.js";
import { groupsOf, slug } from "../../settingsview.js";
import { setContents } from "/static/kp/dist/kp-themes.min.js";
import { NAV } from "./controls.js";

/** @param {import("./ctx.js").Ctx} ctx */
export function buildSide(ctx) {
  const side = h("nav", {
    class: "kp-contents st-side",
    "data-kp-contents": "",
    "data-kp-contents-min": "1",
    // ux-hostsettings-5: a card counts as reached once its top is a little
    // below the sticky bar, so a short card (Account) does not stay
    // highlighted while the next one (This browser) is already in view.
    "data-kp-contents-offset": "160",
    "aria-label": "Settings sections",
  });
  /** The sections the list names: this dashboard's cards, then host.toml's groups. */
  const paintSide = () => {
    const page = ctx.page;
    const gs = page ? groupsOf(page.fields) : [];
    setContents(side, [
      { id: "wc", label: "Working copy" },
      { id: "signin", label: "Sign-in" },
      { id: "home-address", label: "Home address" },
      { id: "account", label: "Account" },
      { id: "browser", label: "This browser" },
      { id: "host-settings", label: "host.toml" },
      ...gs.map((g) => ({
        id: slug(g),
        label: g,
        level: 1,
        count: page?.fields.filter((f) => f.group === g).length ?? "",
      })),
    ]);
  };
  // A group folded shut is opened first, so the press lands on its keys.
  side.addEventListener(
    "click",
    (e) => {
      const link = /** @type {Element} */ (e.target).closest(
        "a.kp-contents__link",
      );
      const id = link?.getAttribute("href")?.slice(1);
      const d = id
        ? /** @type {HTMLDetailsElement | null} */ (
            ctx.groupBox.querySelector(`details#${CSS.escape(id)}`)
          )
        : null;
      if (d) d.open = true;
    },
    true,
  );
  // Live view reaches each entry by its section.
  const markSide = () => {
    for (const a of /** @type {NodeListOf<HTMLAnchorElement>} */ (
      side.querySelectorAll("a.kp-contents__link")
    ))
      if (!a.dataset.drive)
        drivable(a, NAV, (a.getAttribute("href") ?? "").slice(1));
  };
  const sideMarks = new MutationObserver(markSide);
  sideMarks.observe(side, { childList: true, subtree: true });
  ctx.stops.push(() => sideMarks.disconnect());
  return { side, paintSide };
}
