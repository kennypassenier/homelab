// Health (Kenny 2026-09-30, decision "Today, Doctor and Checks → Health"):
// the three pages as one, each its own block. Today (doctor, the fleet
// check, the incidents and the manual checks in one verdict) opens first;
// Doctor's own ~30 s read only starts once its block is opened, so landing
// on Health never pays for a read nobody asked for. `?block=` (set by the
// redirect from the old /app/today, /app/doctor and /app/checks) opens
// that one block instead and scrolls it into view.

import { h } from "../dom.js";
import { mount as mountChecks } from "./checks.js";
import { mount as mountDoctor } from "./doctor.js";
import { mount as mountToday } from "./today.js";

const BLOCKS = /** @type {const} */ ([
  { id: "today", label: "Today", mount: mountToday, lazy: false },
  { id: "doctor", label: "Doctor", mount: mountDoctor, lazy: true },
  { id: "checks", label: "Checks", mount: mountChecks, lazy: false },
]);

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const wanted = new URLSearchParams(location.search).get("block");
  const blocks = h("div", { class: "health-blocks" });
  root.replaceChildren(
    h("h1", null, "Health"),
    // fix-236: the page says what it is, like every other page (rule 8).
    h(
      "p",
      { class: "section-head__desc measured" },
      "Everything about the fleet that needs a look: today's open items, the fleet check against the repository, the host's own doctor and the checks only a person can answer.",
    ),
    blocks,
  );
  return mountHealthBlocks(blocks, wanted, "today");
}

/**
 * feat-shell-4: the three blocks (Today, Doctor, Checks) on their own, for
 * the Inbox, which took over Health in 3.71.0 (`/health?block=x` redirects
 * to `/inbox?kind=x`). `wanted` opens and scrolls to that block; with none
 * wanted, `fallback` opens (null: all folded).
 * @param {HTMLElement} container
 * @param {string | null} wanted
 * @param {string | null} [fallback]
 * @returns {() => void}
 */
export function mountHealthBlocks(container, wanted, fallback = null) {
  const block = BLOCKS.some((b) => b.id === wanted) ? wanted : fallback;

  /** @type {(() => void)[]} */
  const cleanups = [];
  const sections = BLOCKS.map((b) => {
    const body = h("div", { class: "health-block__body" });
    const summary = h("summary", null, h("h2", null, b.label));
    const open = b.id === block;
    const details = /** @type {HTMLDetailsElement} */ (
      h(
        "details",
        {
          class: "health-block",
          id: `health-${b.id}`,
          ...(open ? { open: "" } : {}),
        },
        summary,
        body,
      )
    );
    return { spec: b, body, summary, details, mounted: false };
  });

  container.replaceChildren(...sections.map((s) => s.details));

  // A sub-page paints its own h1 beside its own buttons in one
  // `.title-row` (Doctor's Refresh, Today's Read again) — the right place
  // when it is its own page. Embedded here the h1 is hidden (the
  // `<summary>` already carries the name), which used to leave the button
  // behind on a now near-empty row of its own, under the summary rather
  // than beside it (Kenny, 2026-10-02). Moved into the summary itself,
  // right-aligned next to the name, so opening a block and acting on it is
  // the same line.
  const adoptActions = (
    /** @type {{summary: HTMLElement, body: HTMLElement}} */ s,
  ) => {
    const row = s.body.querySelector(":scope > .title-row");
    if (!row) return;
    for (const child of [...row.children]) {
      if (child.tagName === "H1") continue;
      if (
        child instanceof HTMLButtonElement ||
        child instanceof HTMLAnchorElement
      )
        child.addEventListener("click", (e) => e.stopPropagation());
      s.summary.append(child);
    }
  };

  const mountOne = (/** @type {(typeof sections)[number]} */ s) => {
    if (s.mounted) return;
    s.mounted = true;
    cleanups.push(s.spec.mount(s.body));
    adoptActions(s);
  };

  for (const s of sections) {
    if (s.spec.lazy) {
      if (s.details.open) mountOne(s);
      s.details.addEventListener("toggle", () => {
        if (s.details.open) mountOne(s);
      });
    } else {
      // Today and Checks are a normal fetch, cheap enough to load closed
      // or open; only Doctor's own run is asked to wait for an open block.
      mountOne(s);
    }
  }

  const wantedSection = sections.find((s) => s.spec.id === block);
  if (wanted) wantedSection?.details.scrollIntoView({ block: "start" });

  return () => {
    for (const c of cleanups) c();
  };
}
