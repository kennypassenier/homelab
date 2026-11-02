// The "Host configuration" card's frame: the Show switch, the fold-all
// button and the box the groups are drawn into. The groups themselves are
// drawn by groups.js.

import { drivable } from "../../drivable.js";
import { h } from "../../dom.js";
import { chip, segSwitch } from "../../ui.js";
import { FOLD_ALL, SHOW } from "./controls.js";
import { dataCard } from "./cards.js";

/** @param {import("./ctx.js").Ctx} ctx */
export function buildHostCard(ctx) {
  const showSeg = segSwitch({
    label: "Show",
    items: [
      { value: "all", label: "All", hint: "Show every host setting" },
      {
        value: "changed",
        label: "Changed from default",
        hint: "Show only the settings host.toml changes (or one you changed here)",
      },
      {
        value: "here",
        label: "Editable here",
        hint: "Show only the settings this page may change",
      },
    ],
    value: ctx.S.show,
    onChange: (v) => {
      ctx.S.show = /** @type {import("./ctx.js").Show} */ (v);
      ctx.keepUrl();
      ctx.filter();
    },
    mark: (b, v) => void drivable(b, SHOW, v),
  });
  const foldAll = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        title: "Open or close every group",
      },
      "Open all / close all",
    ),
    FOLD_ALL,
  );
  foldAll.addEventListener("click", () => {
    const ds = [...ctx.groupBox.querySelectorAll("details.st-grp")].filter(
      (d) => !(/** @type {HTMLElement} */ (d).hidden),
    );
    const open = ds.some((d) => !(/** @type {HTMLDetailsElement} */ (d).open));
    for (const d of ds) /** @type {HTMLDetailsElement} */ (d).open = open;
  });
  const groupBox = h("div", {
    class: "st-groups",
    id: "st-groups",
    "data-kp-anchors": "both",
    "data-kp-anchor-levels": "3",
    "data-kp-anchor-open": "on",
  });
  const hsCard = dataCard({
    id: "host-settings",
    title: "Host configuration",
    desc: "Every key of host.toml. Checked with the host's own start-up validation before anything is written.",
    tools: [chip("Waits for Save", { tone: "warn" }), showSeg.el, foldAll],
    data: [groupBox],
  });
  return { showSeg, groupBox, hsCard };
}
