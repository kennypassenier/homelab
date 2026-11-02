// The Host page (redesign-host, release 3.71.0; Kenny approved the demo
// 2026-10-03: ~/.local/share/homelab/redesign-3.71/host.html, implemented
// exactly). It lives under System in the new navigation. Top to bottom:
// the header (live "Reachable · N ms", Run host checks as the primary
// action, Open the console and Host log beside it, the rest in `···`), the
// KPI strip, then two columns — Containers, Host actions grouped by intent
// and Disk on the left (8/12), Connection, About, Templates and Host checks
// on the right (4/12) — and last host.toml as a folded card that shows
// only what the file changes.
//
// Data: the fleet store (load, disk detail, stacks' measured use),
// /data/host/guests (pct list, every 30 s), /data/ping (every 30 s: the
// chip and the latency strip), /data/templates, /data/host-settings,
// /data/disk-growth (the growth line, when Prometheus answers),
// /data/doctor on request, and the action catalog. redesign-host-4: the
// host sends how full the thin pool is, what the guests are promised on
// it, every guest's memory and CPU, its own uptime, the root disk's kind
// and whether its daemon is a signed release; a host older than 3.71.0
// does not, and the page says so instead of inventing a number.
//
// This file assembles the page; each card lives in its own host*.js module
// next to it, built by a create* factory that gets the shared HostEnv.

import { keyRow } from "../ui.js";
import { h } from "../dom.js";
import { refreshOf, registerShortcuts } from "/static/kp/dist/kp-themes.min.js";
import {
  attachBarCells,
  attachBullets,
  attachCardStates,
  attachEntityCards,
  attachFootprints,
  attachHoverCards,
  attachRefresh,
  attachSegmented,
  attachStackbars,
  attachStatusLists,
  attachUptimes,
} from "/static/kp/dist/kp-themes.min.js";
import { GROUP_HOST, unlessDialog } from "../shortcuts.js";
import { current, subscribe } from "../store.js";
import { createAbout, createTemplates } from "./hostabout.js";
import { createActions } from "./hostactions.js";
import { createChecks, createWhenWrong } from "./hostchecks.js";
import { createDisk } from "./hostdisk.js";
import { createGuests } from "./hostguests.js";
import { createHeader } from "./hostheader.js";
import { createKpis } from "./hostkpis.js";
import { createLine } from "./hostline.js";
import { createRefresh } from "./hostrefresh.js";
import { createSettings } from "./hostsettings.js";

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void, focus?: "checks"}} ctx `focus`: the card to open on (redesign-final-h5)
 * @returns {() => void}
 */
export function mount(root, ctx) {
  // `nx-ops`: the shared blocks in the ops-kit look the Host demo uses.
  root.classList.add("hk-page", "nx-ops");
  const abort = new AbortController();
  /** @type {(() => void)[]} */
  const stops = [];
  /** @type {import("./hostenv.js").HostEnv} */
  const env = { abort, stops, ctx };

  /** @type {ReturnType<typeof createChecks>} */
  let checks;
  const header = createHeader(env, {
    onRunChecks: () => {
      checks.card.el.scrollIntoView({ block: "nearest" });
      void checks.load().catch(() => {});
    },
  });
  const kpis = createKpis(env);
  const guests = createGuests(env, { onLoaded: kpis.paint });
  const actions = createActions(env);
  const disk = createDisk(env);
  const line = createLine(env, header);
  const about = createAbout();
  const templates = createTemplates(env);
  checks = createChecks(env, header);
  const whenWrong = createWhenWrong(env);
  const settings = createSettings(env);
  // One refresh control: the figures, the containers and the line to the
  // host are read together.
  const refresh = createRefresh([
    () => guests.load(),
    () => kpis.load().then(() => true),
    () => line.ping(true).then(() => true),
    () => line.loadReach(),
  ]);

  root.replaceChildren(
    header.head.el,
    header.attention.el,
    kpis.el,
    refresh.row,
    guests.card.el,
    h(
      "div",
      { class: "hk-cols" },
      h("div", { class: "hk-col-main" }, actions.card.el, disk.card.el),
      h(
        "div",
        { class: "hk-col-side" },
        line.card.el,
        about.card.el,
        templates.card.el,
        checks.card.el,
        whenWrong.el,
      ),
    ),
    settings.card.el,
    keyRow([
      ["/", "search containers"],
      ["click a header", "sort"],
      ["G H", "go to Host"],
      ["R", "refresh now"],
      ["Esc", "show every container"],
      ["Ctrl K", "any host action"],
    ]),
  );

  stops.push(
    settings.attach(),
    guests.attach(),
    attachSegmented(root),
    attachBarCells(root),
    attachStackbars(root),
    attachUptimes(root),
    attachEntityCards(root),
    attachBullets(root),
    attachStatusLists(root),
    attachCardStates(root),
    attachHoverCards(root),
    attachFootprints(root),
    attachRefresh(root),
  );
  // The line strip shows its busy card from the first paint, not from the
  // first answer (it is in the page now, which the card needs).
  line.paint();

  const render = () => {
    const s = current();
    const f = s.fleet;
    if (!f) return;
    if (header.sub) header.sub.textContent = ` ${f.host.name}`;
    header.daemon.textContent = s.hostVersion
      ? `host daemon ${s.hostVersion}`
      : "";
    header.head.live?.set(f.measured_at);
    kpis.paint();
    disk.paint();
    about.paint();
    line.paint();
    disk.paintGrowth();
  };

  // `/` (the dashboard's search key) searches the containers; Esc shows every
  // container again. kp's engine keeps both out of fields and dialogs.
  const stopKeys = registerShortcuts("page:host", [
    {
      keys: "Escape",
      what: "Show every container again",
      group: GROUP_HOST,
      run: unlessDialog(() => guests.resetFilter()),
    },
  ]);

  const unsub = subscribe(() => {
    render();
    guests.paint();
  });
  render();
  void settings.load().catch(() => {});
  void templates.load().catch(() => {});
  void disk.loadGrowth().catch(() => {});
  // The first read of the figures, the containers and the line goes through
  // the refresh control, so its "updated … ago" is true from the start.
  // Every data area wears its reading state from the first frame, before any
  // answer: the package's busy card, never a blank.
  guests.paint();
  refreshOf(refresh.el)?.now();
  // The doctor's host checks read as the page opens (about 30 s; the card
  // carries the busy card meanwhile and keeps the result until you leave).
  void checks.load().catch(() => {});
  // redesign-final-h5: `/host?section=doctor` (the old Doctor's address)
  // opens this page on its Host checks card and reads them, instead of
  // mounting the retired Doctor page inside it.
  if (ctx.focus === "checks") {
    requestAnimationFrame(() =>
      checks.card.el.scrollIntoView({ block: "start" }),
    );
  }
  return () => {
    abort.abort();
    unsub();
    stopKeys();
    stops.forEach((s) => s());
    root.classList.remove("hk-page", "nx-ops");
  };
}
