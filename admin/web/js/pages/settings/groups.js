// Drawing host.toml's groups (once per read; filtering only hides rows),
// jumping to a key or a section, and the confirm panels of the keys whose
// name must be typed.

import { drivable } from "../../drivable.js";
import { bridge, rowHtml, wire } from "../../settingsform.js";
import { groupsOf, sectionTarget, slug } from "../../settingsview.js";
import { EDIT_KEY, GROUP, HOW_TO } from "./controls.js";

/** @param {string} s */
function escHtml(s) {
  return s.replace(/[&<>"]/g, (c) => `&#${c.charCodeAt(0)};`);
}

/**
 * @param {import("./ctx.js").HostPage} pg
 * @returns {string} the groups' markup
 */
function groupsHtml(pg) {
  const term = "garuda";
  return groupsOf(pg.fields)
    .map((g) => {
      const fs = pg.fields.filter((f) => f.group === g);
      const rows = fs
        .map((f) => rowHtml(f, { path: pg.path, term, howOpen: false }))
        .join("");
      return `<details class="st-grp kp-accordion__item" id="${slug(g)}" data-group="${slug(g)}" data-kp-remember="st-group-${slug(g)}"><summary class="kp-accordion__summary st-grp__sum" data-st-sum="${slug(g)}"><h3 class="st-grp__title" id="h-${slug(g)}">${escHtml(g)}</h3><span class="kp-badge st-grp__count" data-st-count="${slug(g)}">${fs.length} keys</span></summary><div class="kp-accordion__body"><div class="kp-formgrid st-keylist" data-kp-formgrid-label="wide" data-kp-formgrid-lines="lines">${rows}</div></div></details>`;
    })
    .join("");
}

/** Live view reaches each group's summary and each how-to. @param {import("./ctx.js").Ctx} ctx */
function markGroups(ctx) {
  const box = ctx.groupBox;
  for (const s of box.querySelectorAll("[data-st-sum]"))
    drivable(
      /** @type {HTMLElement} */ (s),
      GROUP,
      s.getAttribute("data-st-sum") ?? "",
    );
  for (const s of box.querySelectorAll("summary[id^='how-']"))
    drivable(/** @type {HTMLElement} */ (s), HOW_TO, s.id.replace(/^how-/, ""));
  for (const row of box.querySelectorAll("[data-key]")) {
    const key = row.getAttribute("data-key") ?? "";
    const name = /** @type {HTMLElement} */ (
      row.querySelector("[data-st-name]")
    );
    drivable(name, EDIT_KEY, key);
    name.addEventListener("click", () => ctx.focusKey(key));
  }
}

/** @param {import("./ctx.js").Ctx} ctx */
export async function drawGroups(ctx) {
  if (!ctx.page) return;
  const pg = ctx.page;
  ctx.detachRows?.();
  ctx.unbridge?.();
  ctx.groupBox.innerHTML = groupsHtml(pg);
  markGroups(ctx);
  ctx.detachRows = await wire(ctx.groupBox);
  ctx.unbridge = bridge(ctx.groupBox, (k) =>
    pg.fields.find((f) => f.key === k),
  );
  // The package's own marks (anchors, the group memory) read the groups.
  ctx.filter();
  syncConfirm(ctx);
}

/**
 * The confirm rows show their typed-name panel while the row is changed.
 * @param {import("./ctx.js").Ctx} ctx
 */
export function syncConfirm(ctx) {
  for (const row of ctx.groupBox.querySelectorAll(
    "[data-key][data-st-kind] [data-st-confirm]",
  )) {
    const r = /** @type {HTMLElement} */ (row.closest("[data-key]"));
    const on = r.hasAttribute("data-kp-changed");
    // A staged key is never left inside a folded group (groups start shut).
    if (on) r.closest("details")?.setAttribute("open", "");
    /** @type {HTMLElement} */ (row).hidden = !on;
    if (!on) ctx.typed.delete(r.getAttribute("data-key") ?? "");
  }
}

/** @param {import("./ctx.js").Ctx} ctx @param {string} key */
export function focusKey(ctx, key) {
  const row = ctx.groupBox.querySelector(`[data-key="${CSS.escape(key)}"]`);
  if (!row) return;
  const d = row.closest("details");
  if (d) d.open = true;
  row.scrollIntoView({ block: "center" });
  const c = /** @type {HTMLElement | null} */ (
    row.querySelector(
      "input:not([type=hidden]):not(.kp-sr-only), textarea, select, button",
    )
  );
  c?.focus();
}

/**
 * Opens the section the page was asked to start at, once.
 * @param {import("./ctx.js").Ctx} ctx
 * @returns {() => void}
 */
export function sectionJumper(ctx) {
  let wentTo = false;
  return () => {
    if (wentTo || !ctx.section) return;
    const id = sectionTarget(
      ctx.section,
      ctx.page ? groupsOf(ctx.page.fields) : [],
    );
    if (!id) return;
    wentTo = true;
    const d = /** @type {HTMLDetailsElement | null} */ (
      ctx.groupBox.querySelector(`details#${CSS.escape(id)}`)
    );
    if (d) d.open = true;
    requestAnimationFrame(() =>
      document.getElementById(id)?.scrollIntoView({ block: "start" }),
    );
  };
}
