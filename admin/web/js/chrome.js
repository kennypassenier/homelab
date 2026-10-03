// The shell's furniture around every page: the command palette (Ctrl K,
// feat-overview-3), the help sheet ("?") with the keys it lists
// (feat-overview-8), the theme menu (feat-settings-2), the running-job
// pill, and on a phone the bottom tab bar with its More sheet. All
// kp-themes components; this module only fills them and wires them to the
// router and the store.
//
// feat-shell-1/2/4 (redesign 3.71.0, FLOWS.md §1.2, Kenny approved
// 2026-10-03): the bell is gone (its notices are Inbox rows, counted on the
// bar's Inbox link); the host's questions moved into the Inbox too, with a
// toast when one arrives; the palette finds pages AND actions by intent
// ("update jellyfin", "gateway logs"), grouped Inbox → Do → Go to → Theme;
// the running pill follows a job from every page.

import { act, actionLabel, onAct, startAct } from "./act.js";
import { openAction } from "./actiondialog.js";
import { notify } from "./actui.js";
import { AREAS, moreAreas, phoneTabs } from "./areas.js";
import { openAsks } from "./asks.js";
import {
  actionCommands,
  allCommands,
  editCommands,
  inboxCommands,
  pageCommands,
  registerCommands,
  stackCommands,
  themeCommands,
} from "./commands.js";
import { h } from "./dom.js";
import { drivable } from "./drivable.js";
import {
  changed as notifyInbox,
  countText,
  inboxNow,
  onInbox,
  wireInbox,
} from "./inbox.js";
import { HELP_OPEN, helpPanel } from "./helptour.js";
import { startInboxSources } from "./inboxsources.js";
import { noMatchText, rankCommands } from "./intent.js";
import { finished, stepText } from "./jobs.js";
import { mountJobPanel } from "./jobpanel.js";
import { toastOf } from "./notices.js";
import { openImport } from "./importstack.js";
import { openNewStack } from "./newstack.js";
import { openRollback } from "./rollbackdialog.js";
import { idle, keyAction } from "./shortcuts.js";
import { current, listen, subscribe } from "./store.js";
import {
  OPEN_EVENT,
  RUN_EVENT,
  attachPalettes,
  palette,
} from "/static/kp/js/palette.js";
import {
  THEMES,
  applyTheme,
  currentTheme,
  storeTheme,
} from "/static/kp/js/theme-core.js";
import {
  attachThemePickers,
  themeMenuMarkup,
} from "/static/kp/js/theme-picker.js";
import { attachEffects } from "/static/kp/js/effects.js";

/**
 * @typedef {{navigate: (href: string) => void,
 *   route: () => import("./router.js").Route}} ChromeCtx
 */

/** @param {string} name */
function chooseTheme(name) {
  applyTheme(name);
  storeTheme(name);
}

registerCommands(
  "inbox",
  inboxCommands(() => inboxNow().items),
);
registerCommands("pages", pageCommands);
registerCommands("stacks", stackCommands);
registerCommands("themes", themeCommands(chooseTheme));

/** The palette's dialog; its options are written for every query. */
function paletteDialog() {
  const list = h("ul", {
    class: "kp-palette__list",
    id: "commands-list",
    role: "listbox",
    "aria-label": "Commands",
  });
  const input = /** @type {HTMLInputElement} */ (
    h("input", {
      class: "kp-palette__input",
      type: "text",
      role: "combobox",
      "aria-label": "Search or do anything",
      placeholder:
        "Type a page, a stack or what you want to do, e.g. update gateway",
      "aria-expanded": "true",
      "aria-controls": "commands-list",
      autocomplete: "off",
    })
  );
  const dialog = h(
    "dialog",
    {
      class: "kp-palette",
      id: "commands",
      "data-kp-palette": "",
      "data-kp-hotkey": "k",
      "aria-label": "Search or do anything",
    },
    input,
    list,
    h("p", {
      class: "kp-palette__status",
      role: "status",
      "aria-live": "polite",
    }),
    h(
      "p",
      { class: "nx-palette__foot" },
      "↑ ↓ choose · Enter opens or starts · Esc closes · actions open their dialog first: nothing runs without a confirm",
    ),
  );
  return { dialog, list, input };
}

/**
 * Write the palette's rows for a query: only what answers it, Inbox → Do →
 * Go to → Theme (intent.js); when nothing does, why not.
 * @param {HTMLElement} list
 * @param {string} query
 * @param {string | null} here the stack whose hub is open
 * @returns {import("./commands.js").Command[]}
 */
function fillPalette(list, query, here) {
  const commands = allCommands({
    fleet: current().fleet,
    themes: THEMES,
    theme: currentTheme(),
  });
  const groups = rankCommands(commands, query, { here });
  if (groups.length === 0) {
    const verbs = [
      ...new Set(
        (act.catalog?.actions ?? [])
          .filter((a) => a.target === "stack")
          .map((a) => a.label),
      ),
    ];
    list.replaceChildren(
      h(
        "li",
        { role: "presentation", class: "nx-palette__empty" },
        noMatchText(query, {
          verbs,
          stacks: (current().fleet?.stacks ?? []).map((s) => s.name),
        }),
      ),
    );
    return commands;
  }
  list.replaceChildren(
    ...groups.map((g) =>
      h(
        "li",
        {
          role: "presentation",
          class: "kp-palette__group",
          "data-kp-group": "",
        },
        h("span", { class: "kp-palette__group-label" }, g.group),
        h(
          "ul",
          { role: "group", "aria-label": g.group },
          ...g.commands.map((c) => {
            /** @type {Record<string, string>} */
            const a = {
              class: "kp-palette__option",
              role: "option",
              "data-kp-option": "",
              "data-value": c.id,
            };
            const extra = [
              ...(c.hint
                ? [h("span", { class: "kp-palette__description" }, c.hint)]
                : []),
              ...(c.keys
                ? [h("kbd", { class: "kp-palette__keys" }, c.keys)]
                : []),
            ];
            return h(
              "li",
              { role: "presentation" },
              c.href
                ? h(
                    "a",
                    { ...a, href: c.href, tabindex: "-1" },
                    c.label,
                    ...extra,
                  )
                : h("span", a, c.label, ...extra),
            );
          }),
        ),
      ),
    ),
  );
  return commands;
}

/**
 * Whether a key press belongs to a field or an open dialog, not to us.
 * @param {KeyboardEvent} e
 */
function notOurs(e) {
  if (e.defaultPrevented || e.ctrlKey || e.metaKey || e.altKey) return true;
  const t = /** @type {HTMLElement | null} */ (e.target);
  if (
    t?.closest?.(
      "input, textarea, select, [contenteditable=''], [contenteditable='true']",
    )
  )
    return true;
  return document.querySelector("dialog[open]") != null;
}

/**
 * The jobs the running pill counts: queued or running, oldest first.
 * @param {import("./jobs.js").Job[]} jobs
 */
export const liveJobs = (jobs) =>
  jobs
    .filter((j) => !finished(j.state))
    .sort((a, b) => a.queued_at - b.queued_at);

/**
 * The pill's words: "1 running · Deploy kp-soft · step 4/6".
 * @param {import("./jobs.js").Job[]} live
 * @param {(action: string) => string} label
 */
export function pillText(live, label) {
  if (live.length === 0) return { short: "", long: "" };
  const j = live[0];
  const short = `${live.length} running`;
  const long = ` · ${label(j.action)} ${j.stack} · ${stepText(j)}`;
  return { short, long };
}

/**
 * The running pill (FLOWS.md §1.2): shown only while a job runs; one click
 * opens a drawer with the job's live panel, "Open the full flow" and "All
 * jobs in Activity". Closing the drawer never stops the job.
 * @param {(href: string) => void} navigate
 */
function mountRunningPill(navigate) {
  const short = h("span", { class: "nx-pill__short" });
  const long = h("span", { class: "nx-pill__long" });
  const pill = h(
    "button",
    {
      type: "button",
      class: "nx-pill",
      id: "running-pill",
      hidden: "",
      title: "Follow the running job",
    },
    h("span", { class: "nx-spin", "aria-hidden": "true" }),
    short,
    long,
  );
  const paint = () => {
    const live = liveJobs(act.jobs);
    const t = pillText(
      live,
      (a) => act.catalog?.actions.find((x) => x.action === a)?.label ?? a,
    );
    pill.hidden = live.length === 0;
    short.textContent = t.short;
    long.textContent = t.long;
  };
  pill.addEventListener("click", () => {
    const live = liveJobs(act.jobs);
    if (live.length === 0) return;
    openJobDrawer(live[0].job, live.length, navigate);
  });
  onAct("jobs", paint);
  paint();
  return pill;
}

/**
 * The job drawer: a side panel on a native dialog with the job's own live
 * panel (jobpanel.js).
 * @param {number} jobId
 * @param {number} count how many jobs run now
 * @param {(href: string) => void} navigate
 */
function openJobDrawer(jobId, count, navigate) {
  // redesign-final-c2 (FLOWS.md §1.2): the drawer is named after its job
  // and lists the job's steps above the log tail; the panel lays out in
  // one column at the drawer's own width (app.css `.job-panel` container).
  const panel = mountJobPanel(jobId, { compact: true, steps: true });
  const j = act.jobs.find((x) => x.job === jobId);
  const named = j
    ? `${actionLabel(j.action)} · ${j.stack === "_host" ? "the whole host" : j.stack}`
    : "Running now";
  const close = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost kp-dialog__close",
      "aria-label": "Close",
    },
    "✕",
  );
  const all = h(
    "a",
    { class: "kp-button", href: "/activity?view=running" },
    count > 1 ? `All ${count} jobs in Activity` : "All jobs in Activity",
  );
  const flow = h(
    "a",
    {
      class: "kp-button kp-button--primary",
      href: `/activity?view=running&job=${jobId}`,
    },
    "Open the full flow",
  );
  const d = /** @type {HTMLDialogElement} */ (
    h(
      "dialog",
      { class: "kp-dialog nx-drawer job-drawer", "aria-label": "Running job" },
      h(
        "div",
        { class: "nx-drawer__head" },
        h("h2", { class: "kp-dialog__title" }, named),
        close,
        h(
          "p",
          { class: "section-head__desc" },
          "This job runs on the host; closing this panel or the page does not stop it.",
        ),
      ),
      h("div", { class: "nx-drawer__body" }, panel.element),
      h("div", { class: "nx-drawer__foot" }, all, flow),
    )
  );
  const done = () => d.close();
  close.addEventListener("click", done);
  for (const a of [all, flow])
    a.addEventListener("click", (e) => {
      e.preventDefault();
      done();
      navigate(
        /** @type {HTMLAnchorElement} */ (a).getAttribute("href") ?? "/",
      );
    });
  d.addEventListener("close", () => {
    panel.stop();
    d.remove();
  });
  document.body.append(d);
  d.showModal();
}

/** @param {string} id */
const icon = (id) => {
  /** @type {Record<string, string[]>} */
  const paths = {
    home: ["M3 3h7v7H3z", "M14 3h7v7h-7z", "M3 14h7v7H3z", "M14 14h7v7h-7z"],
    inbox: ["M4 13h4l2 3h4l2-3h4", "M5 5h14l1 8v6H4v-6z"],
    overview: ["m12 3 9 5-9 5-9-5z", "m3 13 9 5 9-5"],
    activity: ["M3 12h4l3-7 4 14 3-7h4"],
    more: ["M5 12h.01", "M12 12h.01", "M19 12h.01"],
  };
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("aria-hidden", "true");
  for (const d of paths[id] ?? []) {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    svg.append(p);
  }
  return svg;
};

/**
 * The phone's bottom tab bar (FLOWS.md §1.1 "Phone"): the four frequent
 * areas within thumb reach, then More, whose sheet holds Backups, System,
 * Search and Help. Hidden above 60 rem by app.css.
 * @param {ChromeCtx} ctx
 * @param {() => void} openPalette
 * @param {() => void} openHelp
 * @returns {{el: HTMLElement, paint: () => void}}
 */
function mountTabBar(ctx, openPalette, openHelp) {
  const badge = h("span", { class: "kp-badge nx-count", hidden: "" });
  const tabs = phoneTabs().map((a) =>
    h(
      "a",
      { href: a.href, "data-area": a.id, class: "nx-tabbar__tab" },
      icon(a.id),
      h("span", null, a.label),
      ...(a.id === "inbox" ? [badge] : []),
    ),
  );
  const more = h(
    "button",
    { type: "button", class: "nx-tabbar__tab", "data-area": "more" },
    icon("more"),
    h("span", null, "More"),
  );
  const el = h(
    "nav",
    { class: "nx-tabbar", "aria-label": "Areas" },
    ...tabs,
    more,
  );
  more.addEventListener("click", () => {
    const item = (
      /** @type {string} */ title,
      /** @type {string} */ what,
      /** @type {() => void} */ go,
    ) => {
      const b = h(
        "button",
        { type: "button", class: "nx-action" },
        h("strong", null, title),
        h("span", null, what),
      );
      b.addEventListener("click", () => {
        d.close();
        go();
      });
      return b;
    };
    const close = h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--ghost kp-dialog__close",
        "aria-label": "Close",
      },
      "✕",
    );
    const d = /** @type {HTMLDialogElement} */ (
      h(
        "dialog",
        { class: "kp-dialog nx-drawer nx-sheet", "aria-label": "More" },
        h(
          "div",
          { class: "nx-drawer__head" },
          h("h2", { class: "kp-dialog__title" }, "More"),
          close,
          h(
            "p",
            { class: "section-head__desc" },
            "The two areas you need less often, plus search and help.",
          ),
        ),
        h(
          "div",
          { class: "nx-drawer__body nx-actions" },
          ...moreAreas().map((a) =>
            item(a.label, a.what, () => ctx.navigate(a.href)),
          ),
          item(
            "Search or do anything",
            "Pages, stacks and actions in one box",
            openPalette,
          ),
          item(
            "Help and the words",
            "What a stack, a deploy or a snapshot is",
            openHelp,
          ),
        ),
      )
    );
    close.addEventListener("click", () => d.close());
    d.addEventListener("close", () => d.remove());
    document.body.append(d);
    d.showModal();
  });
  const paint = () => {
    const r = ctx.route();
    const areaId =
      r.page === "stack"
        ? "overview"
        : (AREAS.find((a) => a.id === r.page)?.id ?? null);
    for (const t of [...tabs, more]) t.removeAttribute("aria-current");
    const hit = tabs.find((t) => t.dataset.area === areaId);
    if (hit) hit.setAttribute("aria-current", "page");
    else if (moreAreas().some((a) => a.id === areaId))
      more.setAttribute("aria-current", "page");
    const n = inboxNow().items.length;
    badge.textContent = countText(n);
    badge.hidden = n === 0;
  };
  onInbox(paint);
  paint();
  return { el, paint };
}

/**
 * Put the palette, the help sheet, the theme menu, the running pill and the
 * phone tab bar in place.
 * @param {{nav: HTMLElement}} where
 * @param {ChromeCtx} ctx
 * @returns {{repaint: () => void}} repaint what depends on the route
 */
export function mountChrome(where, ctx) {
  const { dialog, list, input } = paletteDialog();
  // redesign-flows-3: the Help panel and the first-visit tour (helptour.js).
  const sheet = helpPanel();
  const trigger = h(
    "div",
    { class: "kp-nav__search" },
    h(
      "button",
      {
        type: "button",
        class: "kp-nav__search-trigger nx-search-trigger",
        "data-kp-palette-open": "commands",
        title: "Search pages, stacks and actions, or start one (Ctrl K)",
      },
      h("span", { class: "nx-search-trigger__text" }, "Search or do anything…"),
      h("kbd", { class: "kp-palette__keys", "data-kp-palette-keys": "" }),
    ),
  );
  // Review item 15: the bar's ? is the Live view control help-open.
  const help = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--ghost help-button",
        "data-kp-palette-open": "shortcuts",
        "aria-label": "Help, words and shortcuts",
        title: "Help, words and shortcuts (?)",
      },
      "?",
    ),
    HELP_OPEN,
  );
  const themes = h("div", { class: "theme-slot" });
  themes.innerHTML = themeMenuMarkup({
    id: "theme-menu",
    label: "Choose a theme",
  });
  // Milestone act: the catalog, the jobs, the notices and the schedules,
  // and every action as a palette command (feat-stacks-4).
  startAct();
  wireInbox(() => ({
    asks: current().asks,
    notices: act.notices?.notices ?? (act.failed.notices ? [] : null),
  }));
  // redesign-flows-2: the Inbox's slower sources (updates, setup, checks,
  // Today), so the bar's counter is the Inbox's row count on every page.
  startInboxSources();
  subscribe(() => notifyInbox());
  onAct("notices", () => notifyInbox());
  // A question's countdown moves on its own; re-count once a second only
  // while one is open.
  setInterval(() => {
    if (openAsks(current().asks, Date.now() / 1000).length) notifyInbox();
  }, 1000);
  const here = () => {
    const r = ctx.route();
    return r.page === "stack" ? r.name : null;
  };
  registerCommands(
    "actions",
    actionCommands(
      () => act.catalog,
      here,
      (stack, action) => void openAction(stack, action, { openRollback }),
    ),
  );
  // Milestone edit: a new stack, deploying every change, each stack's
  // settings, secrets and firewall.
  registerCommands(
    "edit",
    editCommands(
      here,
      () => void openNewStack(ctx.navigate),
      () => void openImport(ctx.navigate),
    ),
  );
  const pill = mountRunningPill(ctx.navigate);
  where.nav.append(trigger, pill, help, themes);
  document.body.append(dialog, sheet);
  attachThemePickers(themes);
  // The themes' pointer effects (the dark theme's cursor glow) follow the
  // mouse only once attached (Kenny, 2026-09-29 05:53).
  attachEffects(document);
  // feat-shell-2: the rows are written per query by intent.js's ranking,
  // so kp's own filter keeps every row it is given.
  attachPalettes(document, { hotkey: "k", sheetKey: "?", match: () => true });

  /** @type {import("./commands.js").Command[]} */
  let commands = fillPalette(list, "", here());
  const refill = () => {
    commands = fillPalette(list, input.value, here());
    palette(dialog)?.refresh();
  };
  input.addEventListener("input", refill);
  dialog.addEventListener(OPEN_EVENT, (e) => {
    if (!(/** @type {CustomEvent} */ (e).detail?.open)) return;
    refill();
  });
  dialog.addEventListener(RUN_EVENT, (e) => {
    const value = /** @type {CustomEvent} */ (e).detail?.value;
    const c = commands.find((x) => x.id === value);
    if (!c) return;
    e.preventDefault();
    palette(dialog)?.close();
    if (c.href) ctx.navigate(c.href);
    else c.run?.();
  });

  const tabbar = mountTabBar(
    ctx,
    () => palette(dialog)?.open(),
    () => palette(sheet)?.open(),
  );
  document.body.append(tabbar.el);

  // The notices the bell used to count: a pop-up one still toasts, and
  // its Open goes to the Inbox (or the job's own flow).
  listen("notification", (ev) => {
    if (!ev?.pop_up || !ev.notice) return;
    const t = toastOf(ev.notice);
    const job = ev.notice.job;
    notify(
      t.text,
      /** @type {"success" | "warning" | "info" | "error"} */ (t.tone),
      job
        ? {
            label: "Open the job",
            onClick: () => ctx.navigate(`/activity?view=running&job=${job}`),
          }
        : { label: "Open the Inbox", onClick: () => ctx.navigate("/inbox") },
    );
  });
  // A new host question toasts once, wherever you are (the strip under
  // the bar it used to be is the Inbox's top row now).
  /** @type {Set<string>} */
  const askedSeen = new Set();
  const onAsks = () => {
    for (const a of openAsks(current().asks, Date.now() / 1000)) {
      const k = `${a.boot ?? ""}:${a.id}`;
      if (askedSeen.has(k)) continue;
      askedSeen.add(k);
      notify(
        `The host is asking: ${a.op} is waiting at "${a.step}"`,
        "warning",
        {
          label: "Answer in the Inbox",
          onClick: () => ctx.navigate("/inbox"),
        },
      );
    }
  };
  subscribe(onAsks);

  let chord = idle();
  document.addEventListener("keydown", (e) => {
    if (notOurs(e)) return;
    const r = keyAction(chord, e.key, Date.now(), ctx.route());
    chord = r.state;
    const a = r.action;
    if (!a) return;
    e.preventDefault();
    if ("navigate" in a) ctx.navigate(a.navigate);
    else {
      const search = /** @type {HTMLInputElement | null} */ (
        document.querySelector("#page .nx-search, #page .kp-datatable__search")
      );
      search?.focus();
    }
  });
  return { repaint: () => tabbar.paint() };
}
