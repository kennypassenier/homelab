// The Host page's header: the live "Reachable · N ms" chip, the daemon
// version, Run host checks as the primary action, Open the console and
// Host log beside it, the rest in `···`; and the attention band under it.

import { h } from "../dom.js";
import { declare, drivable } from "../drivable.js";
import { current } from "../store.js";
import { attentionBand, dot, moreMenu, pageHeader } from "../ui.js";

const RUN_CHECKS = declare({
  id: "run-host-checks",
  page: "host",
  opens: "run",
  what: "read the doctor's host-level checks now (about 30 s)",
  // kit round (chassis-rs 3.9.0): the Host checks card's own Run checks was a
  // second button for the same read; the header's one button took its place.
  was: ["read-host-checks"],
});
const MORE = declare({
  id: "host-more",
  page: "host",
  opens: "run",
  what: "open the header's menu: presets and the runbook",
});

/**
 * @param {import("./hostenv.js").HostEnv} env
 * @param {{onRunChecks: () => void}} hooks
 */
export function createHeader(env, { onRunChecks }) {
  const { stops } = env;
  const reach = h(
    "span",
    { class: "hk-chip", id: "host-reach" },
    dot(""),
    "Checking the line…",
  );
  const daemon = h("span", { id: "host-daemon" });
  const runChecks = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--primary",
        title: "Read the doctor's host-level checks (about 30 s)",
        onclick: onRunChecks,
      },
      "Run host checks",
    ),
    RUN_CHECKS,
  );
  const menu = moreMenu({
    label: "More: presets, download the runbook",
    items: [
      {
        label: "Presets",
        hint: "The stack recipes a new stack starts from",
        href: "/presets",
      },
      {
        label: "Download the runbook",
        hint: "The disaster-recovery runbook, as Markdown",
        href: "/data/download/runbook",
        download: "DR_RUNBOOK.md",
      },
    ],
    drive: { id: MORE },
  });
  stops.push(menu.stop);
  const head = pageHeader({
    title: "Host",
    sub: current().fleet?.host.name ?? "",
    desc: "The Proxmox machine every stack runs on: how loaded it is, what runs on it, and the actions that act on the host itself.",
    meta: [reach, daemon],
    live: "measured",
    actions: [
      h(
        "a",
        {
          class: "kp-button",
          href: "/console",
          title: "Open a root shell on the host in this browser",
        },
        "Open the console",
      ),
      h(
        "a",
        {
          class: "kp-button",
          href: "/activity?view=host-log",
          title: "Follow the host daemon's own log, live",
        },
        "Host log",
      ),
    ],
    primary: runChecks,
    more: menu.el,
  });
  const sub = /** @type {HTMLElement | null} */ (
    head.title.querySelector(".nx-head-sub")
  );
  const attention = attentionBand();
  return { head, reach, daemon, runChecks, sub, attention };
}
