// feat-shell-1 (redesign 3.71.0, FLOWS.md §1 "System"): the rarely needed
// area's landing. redesign-final-h4: as the approved flows/system.html —
// four headed groups (The host · The fleet as a whole · Set up · Tools),
// the host's live line in the header and on the Host card, and the
// disaster runbook among the tools. Every page's address comes from
// areas.js's list, so this page and the palette never disagree. Its
// problems surface in the Inbox on their own, so a person who never opens
// System misses nothing.

import { SUB_PAGES } from "../areas.js";
import { fetchJson, h } from "../dom.js";
import { current, subscribe } from "../store.js";
import { SYSTEM_GROUPS, hostLine } from "../systemview.js";
import { dot, ensureStyle, pageHeader, section } from "../ui.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/system.css");
  const state = h("span", { class: "sy-state" });
  const head = pageHeader({
    title: "System",
    titleMeta: [state],
    desc: "The machine underneath and the settings that apply to every stack. Day to day you rarely need this: problems from here show up in the Inbox on their own.",
  });
  /** @type {HTMLElement | null} */
  let liveLine = null;
  const hrefOf = (/** @type {string} */ id) =>
    SUB_PAGES.find((s) => s.id === id)?.href ?? "/system";
  const groups = SYSTEM_GROUPS.map((g) => {
    const card = section({ title: g.title, desc: g.desc });
    card.el.classList.add("sy-group");
    card.body.append(
      h(
        "nav",
        { class: "nx-actions sy-links", "aria-label": g.title },
        ...g.cards.map((c) => {
          const line = c.id === "host" ? h("span", { class: "sy-kpi" }) : null;
          if (line) liveLine = line;
          return h(
            "a",
            {
              class: "nx-action",
              href: c.href ?? hrefOf(c.id),
              ...(c.download ? { download: c.download } : {}),
              "data-card": c.id,
            },
            h("strong", null, c.label),
            h("span", null, c.what),
            line,
          );
        }),
      ),
    );
    return card;
  });
  root.replaceChildren(head.el, ...groups.map((g) => g.el));

  /** @type {any} */
  let versions = null;
  const paint = () => {
    const s = current();
    const l = hostLine(s.fleet, versions, s.up === false);
    state.replaceChildren(dot(l.tone, l.word));
    if (liveLine) {
      liveLine.textContent = l.line;
      liveLine.hidden = !l.line;
    }
  };
  const off = subscribe(paint);
  paint();
  let alive = true;
  void fetchJson("/data/versions", "the versions").then((r) => {
    if (!alive || !r.ok) return;
    versions = r.body;
    paint();
  });
  return () => {
    alive = false;
    off();
    groups.forEach((g) => g.stop());
  };
}
