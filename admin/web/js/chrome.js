// The shell's furniture around every page: the command palette (Ctrl K,
// feat-overview-3), the shortcut sheet ("?") and the keys it lists
// (feat-overview-8), the theme menu (feat-settings-2) and the host's open
// questions (feat-ops-2). All kp-themes components; this module only fills
// them and wires them to the router and the store.

import { act, onAct, startAct } from "./act.js";
import { openAction } from "./actiondialog.js";
import { notify } from "./actui.js";
import { answerBody, askView, openAsks } from "./asks.js";
import {
  actionCommands,
  allCommands,
  grouped,
  pageCommands,
  registerCommands,
  stackCommands,
  themeCommands,
} from "./commands.js";
import { h } from "./dom.js";
import { bell, snoozeState, toastOf } from "./notices.js";
import { openRollback } from "./rollbackdialog.js";
import { SHORTCUTS, idle, keyAction } from "./shortcuts.js";
import { current, listen, setAsks, subscribe } from "./store.js";
import {
  OPEN_EVENT,
  RUN_EVENT,
  attachPalettes,
  isMac,
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

/**
 * @typedef {{navigate: (href: string) => void,
 *   route: () => import("./router.js").Route}} ChromeCtx
 */

/** @param {string} name */
function chooseTheme(name) {
  applyTheme(name);
  storeTheme(name);
}

registerCommands("pages", pageCommands);
registerCommands("stacks", stackCommands);
registerCommands("themes", themeCommands(chooseTheme));

/** @param {string} keys */
const shown = (keys) => (keys === "Ctrl K" && isMac() ? "⌘ K" : keys);

/** The palette's dialog; its options are written each time it opens. */
function paletteDialog() {
  const list = h("ul", {
    class: "kp-palette__list",
    id: "commands-list",
    role: "listbox",
    "aria-label": "Commands",
  });
  const dialog = h(
    "dialog",
    {
      class: "kp-palette",
      id: "commands",
      "data-kp-palette": "",
      "data-kp-hotkey": "k",
      "aria-label": "Commands",
    },
    h("input", {
      class: "kp-palette__input",
      type: "text",
      role: "combobox",
      "aria-label": "Go to a page, a stack, an action or a theme",
      placeholder: "Go to a page, a stack, an action or a theme…",
      "aria-expanded": "true",
      "aria-controls": "commands-list",
      autocomplete: "off",
    }),
    list,
    h("p", {
      class: "kp-palette__status",
      role: "status",
      "aria-live": "polite",
    }),
  );
  return { dialog, list };
}

/** @param {HTMLElement} list @returns {import("./commands.js").Command[]} */
function fillPalette(list) {
  const commands = allCommands({
    fleet: current().fleet,
    themes: THEMES,
    theme: currentTheme(),
  });
  list.replaceChildren(
    ...grouped(commands).map((g) =>
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
            if (c.keys) a["data-kp-keys"] = c.keys;
            const hint = c.hint
              ? [h("span", { class: "kp-palette__description" }, c.hint)]
              : [];
            return h(
              "li",
              { role: "presentation" },
              c.href
                ? h("a", { ...a, href: c.href }, c.label, ...hint)
                : h("span", a, c.label, ...hint),
            );
          }),
        ),
      ),
    ),
  );
  return commands;
}

/** The "?" sheet, drawn from the same list the keys obey. */
function shortcutSheet() {
  return h(
    "dialog",
    {
      class: "kp-shortcuts",
      id: "shortcuts",
      "data-kp-shortcuts": "",
      "aria-label": "Keyboard shortcuts",
    },
    h("h2", { class: "kp-dialog__title" }, "Keyboard shortcuts"),
    ...SHORTCUTS.map((g) =>
      h(
        "section",
        { class: "kp-shortcuts__group" },
        h("h3", { class: "kp-shortcuts__group-label" }, g.group),
        h(
          "dl",
          { class: "kp-shortcuts__list" },
          ...g.shortcuts.map((s) =>
            h(
              "div",
              { class: "kp-shortcuts__row" },
              h(
                "dt",
                null,
                h("kbd", { class: "kp-palette__keys" }, shown(s.keys)),
              ),
              h("dd", null, s.what),
            ),
          ),
        ),
      ),
    ),
  );
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
 * The host's questions, above every page (feat-ops-2).
 * @param {HTMLElement} region
 */
function mountAsks(region) {
  /** @type {Map<string, {box: HTMLElement, left: HTMLElement}>} */
  const shownAsks = new Map();

  /** @param {import("./asks.js").Ask} a @param {boolean} allow @param {HTMLElement} box */
  const answer = async (a, allow, box) => {
    box.querySelectorAll("button").forEach((b) => (b.disabled = true));
    const note = /** @type {HTMLElement} */ (box.querySelector(".ask-note"));
    note.textContent = allow ? "Sending: allow…" : "Sending: stop…";
    /** @type {any} */
    let body = null;
    let ok = false;
    try {
      const r = await fetch("/data/asks/answer", {
        method: "POST",
        headers: {
          "content-type": "application/json",
          accept: "application/json",
        },
        body: JSON.stringify(answerBody(a, allow)),
      });
      ok = r.ok;
      body = await r.json().catch(() => null);
    } catch {
      body = { why: "the dashboard did not answer" };
    }
    if (ok) {
      note.textContent = allow
        ? "Allowed. The operation goes on."
        : "Stopped. The operation stops here.";
      setAsks(
        current().asks.filter((x) => !(x.id === a.id && x.boot === a.boot)),
      );
      return;
    }
    note.textContent = `Not sent: ${body?.why ?? "unknown error"}${body?.fix ? `. ${body.fix}` : ""}`;
    box.querySelectorAll("button").forEach((b) => (b.disabled = false));
  };

  const render = () => {
    const now = Date.now() / 1000;
    const open = openAsks(current().asks, now);
    const keys = new Set(open.map((a) => askView(a, now).key));
    for (const [k, v] of shownAsks)
      if (!keys.has(k)) {
        v.box.remove();
        shownAsks.delete(k);
      }
    for (const a of open) {
      const v = askView(a, now);
      const known = shownAsks.get(v.key);
      if (known) {
        known.left.textContent = v.left;
        known.box.dataset.urgent = String(v.urgent);
        continue;
      }
      const left = h("p", { class: "measured ask-left" }, v.left);
      const allow = h(
        "button",
        { type: "button", class: "kp-button" },
        "Allow",
      );
      const stop = h(
        "button",
        { type: "button", class: "kp-button kp-button--destructive" },
        "Stop",
      );
      const box = h(
        "div",
        {
          class: "kp-alert kp-alert--warning ask",
          role: "alert",
          "data-ask": v.key,
        },
        h("strong", null, v.title),
        h("p", null, v.what),
        h(
          "dl",
          { class: "facts ask-consequences" },
          h("dt", null, "Allow"),
          h("dd", null, v.ifAllowed),
          h("dt", null, "Stop"),
          h("dd", null, v.ifStopped),
        ),
        h("div", { class: "ask-buttons" }, allow, stop),
        left,
        h("p", { class: "ask-note", role: "status", "aria-live": "polite" }),
      );
      allow.addEventListener("click", () => void answer(a, true, box));
      stop.addEventListener("click", () => void answer(a, false, box));
      region.append(box);
      shownAsks.set(v.key, { box, left });
    }
    region.hidden = region.childElementCount === 0;
  };
  subscribe(render);
  setInterval(render, 1000);
  render();
}

/**
 * The bell (feat-overview-5): the unread count in the bar, a link to the
 * notification centre, and a kp toast for every notice that pops up.
 * @param {(href: string) => void} navigate
 */
function mountBell(navigate) {
  const count = h("span", {
    class: "kp-badge bell-count",
    "aria-hidden": "true",
  });
  const link = h(
    "a",
    {
      class: "kp-button kp-button--ghost bell",
      href: "/app/notifications",
      id: "bell",
    },
    bellIcon(),
    count,
  );
  const paint = () => {
    const snap = act.notices;
    const b = bell(snap?.unread ?? 0);
    count.textContent = b.count;
    count.hidden = b.count === "";
    const sn = snoozeState(snap?.settings, Date.now() / 1000);
    link.setAttribute("aria-label", sn.on ? `${b.label}; ${sn.text}` : b.label);
    link.title = sn.on ? sn.text : b.label;
    link.dataset.snoozed = String(sn.on);
  };
  onAct("notices", paint);
  paint();
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
            onClick: () => navigate(`/app/jobs?job=${job}`),
          }
        : { label: "Open", onClick: () => navigate("/app/notifications") },
    );
  });
  return link;
}

/** A bell, drawn in the text colour. */
function bellIcon() {
  const ns = "http://www.w3.org/2000/svg";
  const svg = document.createElementNS(ns, "svg");
  svg.setAttribute("viewBox", "0 0 24 24");
  svg.setAttribute("width", "18");
  svg.setAttribute("height", "18");
  svg.setAttribute("aria-hidden", "true");
  svg.setAttribute("fill", "none");
  svg.setAttribute("stroke", "currentColor");
  svg.setAttribute("stroke-width", "2");
  svg.setAttribute("stroke-linecap", "round");
  svg.setAttribute("stroke-linejoin", "round");
  for (const d of [
    "M6 8a6 6 0 0 1 12 0c0 7 3 9 3 9H3s3-2 3-9",
    "M10.3 21a1.94 1.94 0 0 0 3.4 0",
  ]) {
    const p = document.createElementNS(ns, "path");
    p.setAttribute("d", d);
    svg.append(p);
  }
  return svg;
}

/**
 * Put the palette, the sheet, the theme menu and the questions in place.
 * @param {{nav: HTMLElement, asks: HTMLElement}} where
 * @param {ChromeCtx} ctx
 */
export function mountChrome(where, ctx) {
  const { dialog, list } = paletteDialog();
  const sheet = shortcutSheet();
  const trigger = h(
    "div",
    { class: "kp-nav__search" },
    h(
      "button",
      {
        type: "button",
        class: "kp-nav__search-trigger",
        "data-kp-palette-open": "commands",
      },
      "Go to… ",
      h("kbd", { class: "kp-palette__keys", "data-kp-palette-keys": "" }),
    ),
  );
  const help = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--ghost help-button",
      "data-kp-palette-open": "shortcuts",
      "aria-label": "Keyboard shortcuts",
      title: "Keyboard shortcuts (?)",
    },
    "?",
  );
  const themes = h("div", { class: "theme-slot" });
  themes.innerHTML = themeMenuMarkup({
    id: "theme-menu",
    label: "Choose a theme",
  });
  // Milestone act: the catalog, the jobs, the notices and the schedules,
  // and every action as a palette command (feat-stacks-4).
  startAct();
  registerCommands(
    "actions",
    actionCommands(
      () => act.catalog,
      () => {
        const r = ctx.route();
        return r.page === "stack" ? r.name : null;
      },
      (stack, action) => void openAction(stack, action, { openRollback }),
    ),
  );
  where.nav.append(trigger, mountBell(ctx.navigate), help, themes);
  document.body.append(dialog, sheet);
  attachThemePickers(themes);
  attachPalettes(document, { hotkey: "k", sheetKey: "?" });

  /** @type {import("./commands.js").Command[]} */
  let commands = fillPalette(list);
  dialog.addEventListener(OPEN_EVENT, (e) => {
    if (!(/** @type {CustomEvent} */ (e).detail?.open)) return;
    commands = fillPalette(list);
    palette(dialog)?.refresh();
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
        document.querySelector("#page .kp-datatable__search")
      );
      search?.focus();
    }
  });

  mountAsks(where.asks);
}
