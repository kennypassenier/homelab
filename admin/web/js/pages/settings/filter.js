// The search and the Show choice: hide the rows, groups and counts they rule
// out, mark the match in the key's name, and offer a way out when nothing
// matches.

import { drivable } from "../../drivable.js";
import { h } from "../../dom.js";
import { editable } from "../../editforms.js";
import { fieldShown, foundText, groupsOf, slug } from "../../settingsview.js";
import { highlight } from "../../ui.js";
import { CLEAR } from "./controls.js";

/**
 * The search's match is marked in the key's name.
 * @param {string} q @param {HTMLElement} row
 * @param {import("./ctx.js").HostField} f @param {boolean} ok
 */
function highlightName(q, row, f, ok) {
  const name = row.querySelector("[data-st-name]");
  if (!name || !ok) return;
  name.replaceChildren(
    .../** @type {(Node | string)[]} */ ([highlight(f.label, q) ?? ""]),
  );
}

/**
 * Show only what the search and the Show choice let through.
 * @param {import("./ctx.js").Ctx} ctx
 * @param {import("./ctx.js").HostPage} pg
 * @param {HTMLDetailsElement} d
 * @param {string} g
 * @param {boolean} narrowed
 * @returns {number} the keys of the group still shown
 */
function filterGroup(ctx, pg, d, g, narrowed) {
  const all = pg.fields.filter((f) => f.group === g);
  let shown = 0;
  for (const f of all) {
    const row = /** @type {HTMLElement | null} */ (
      d.querySelector(`[data-key="${CSS.escape(f.key)}"]`)
    );
    if (!row) continue;
    const ok = fieldShown(
      f,
      ctx.S.q,
      ctx.S.show,
      editable,
      (k) =>
        ctx.groupBox
          .querySelector(`[data-key="${CSS.escape(k)}"]`)
          ?.hasAttribute("data-kp-changed") ?? false,
    );
    row.hidden = !ok;
    if (ok) shown += 1;
    highlightName(ctx.S.q, row, f, ok);
  }
  d.hidden = shown === 0;
  if (narrowed && shown > 0) d.open = true;
  const badge = d.querySelector("[data-st-count]");
  if (badge)
    badge.textContent =
      shown === all.length
        ? `${all.length} ${all.length === 1 ? "key" : "keys"}`
        : `${shown} of ${all.length} keys`;
  return shown;
}

/** @param {import("./ctx.js").Ctx} ctx */
function noMatch(ctx) {
  const clear = drivable(
    h("button", { type: "button", class: "kp-button" }, "Clear the search"),
    CLEAR,
  );
  clear.addEventListener("click", () => {
    ctx.S.q = "";
    if (ctx.tb.search) ctx.tb.search.value = "";
    ctx.S.show = "all";
    ctx.showSeg.set("all");
    ctx.keepUrl();
    ctx.filter();
  });
  return h(
    "div",
    { class: "kp-empty st-nomatch" },
    h("p", { class: "kp-empty__title" }, "No setting matches"),
    h(
      "p",
      { class: "kp-empty__body" },
      "Nothing in host.toml has these words in its name, key or help.",
    ),
    clear,
  );
}

/** Show only what the search and the Show choice let through. @param {import("./ctx.js").Ctx} ctx */
export function filter(ctx) {
  if (!ctx.page) return;
  const pg = ctx.page;
  const narrowed = !!ctx.S.q || ctx.S.show !== "all";
  let total = 0;
  for (const g of groupsOf(pg.fields)) {
    const d = /** @type {HTMLDetailsElement | null} */ (
      ctx.groupBox.querySelector(`details#${CSS.escape(slug(g))}`)
    );
    if (!d) continue;
    total += filterGroup(ctx, pg, d, g, narrowed);
  }
  ctx.found.textContent = foundText(total, pg.fields.length, narrowed);
  const empty = ctx.groupBox.querySelector(".st-nomatch");
  if (total === 0 && !empty) ctx.groupBox.append(noMatch(ctx));
  else if (total > 0 && empty) empty.remove();
}
