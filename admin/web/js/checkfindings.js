// checks-notice-link-1 (Kenny, 2026-10-24: "nightly check: 2 broken 12 drift
// ... nutteloos dus"): the findings of the latest fleet check round, every
// broken and drift one with its remedy, the noted ones folded. The nightly
// notice's "See what happened" opens it (`/inbox?section=check-findings`,
// core/src/notify.rs `page::CHECKS`), and so does the old address
// (`/health?block=checks`, router.js). The round is read from the host's
// fleet check (`/data/fleet-check`), the same read the Inbox's check rows
// come from: the dashboard keeps the last answer and shows it at once.

import { declare, drivable } from "./drivable.js";
import { countOf } from "./ui.js";
import { findingsView } from "./findingsview.js";
import { h, slowRead } from "./dom.js";
import { stage } from "./stage.js";
import { switchCheck } from "./editpanels.js";
import { setCardState } from "/static/kp/js/card-state.js";

const FOLD = declare({
  id: "inbox-check-findings",
  page: "needs-you",
  opens: "view",
  what: "fold or unfold the latest fleet check round: every broken and drift finding with its remedy, the noted ones folded",
});
const TURN_OFF = declare({
  id: "inbox-finding-turn-off",
  page: "needs-you",
  opens: "dialog",
  row: "<stack>/<app>/<check name>",
  what: "a finding's Turn off…: asks a short reason and writes off: <reason> on that check or probe in the stack's checks.yml (a commit)",
  shows: "on a finding about one check or probe of an app",
});

/**
 * A finding's words with an upstream address (`github.com/owner/repo`) as a
 * working link, never as text that looks like one.
 * @param {string} text
 * @returns {(string | HTMLElement)[]}
 */
export function linked(text) {
  /** @type {(string | HTMLElement)[]} */
  const out = [];
  let at = 0;
  for (const m of text.matchAll(/\bgithub\.com\/[\w.-]+\/[\w.-]+/g)) {
    const start = m.index ?? 0;
    if (start > at) out.push(text.slice(at, start));
    out.push(
      h(
        "a",
        {
          href: `https://${m[0]}`,
          target: "_blank",
          rel: "noopener noreferrer",
        },
        m[0],
      ),
    );
    at = start + m[0].length;
  }
  if (at < text.length) out.push(text.slice(at));
  return out;
}

/**
 * One finding as a row of the Inbox's own list: its tone, what it is about
 * and says, what to do, and the buttons it can offer.
 * @param {ReturnType<typeof findingsView>["broken"][number]} r
 * @param {() => void} again read the round again after a change
 */
function findingRow(r, again) {
  const key = `${r.subject}: ${r.what}`;
  return h(
    "li",
    { "data-key": `finding:${key}`, "data-kp-state": "new" },
    h(
      "div",
      { class: "kp-action-list__main" },
      h(
        "strong",
        { class: "kp-action-list__title" },
        h(
          "span",
          { class: `kp-badge kp-badge--${badgeTone(r)}` },
          r.badge.label,
        ),
        " ",
        r.subject,
      ),
      h("span", { class: "kp-action-list__text" }, ...linked(r.what)),
      h("span", { class: "kp-action-list__text" }, `What to do: ${r.remedy}`),
    ),
    h(
      "div",
      { class: "kp-row-actions" },
      ...(r.ref
        ? [
            (() => {
              const ref = r.ref;
              const b = drivable(
                h(
                  "button",
                  {
                    type: "button",
                    class: "kp-button kp-button--sm",
                    title:
                      "Switch this check off (`off: true`) in the stack's checks.yml. The nightly round skips it.",
                  },
                  "Turn off",
                ),
                TURN_OFF,
                `${ref.stack}/${ref.app}/${ref.name}`,
              );
              b.addEventListener(
                "click",
                () =>
                  void switchCheck({
                    stack: ref.stack,
                    app: ref.app,
                    name: ref.name,
                    onCommitted: again,
                  }),
              );
              return b;
            })(),
          ]
        : []),
    ),
  );
}

/** @param {{badge: {tone: string}}} r */
const badgeTone = (r) =>
  r.badge.tone === "bad"
    ? "destructive"
    : r.badge.tone === "warn"
      ? "warning"
      : "success";

/**
 * The fold for the Inbox page.
 * @param {AbortSignal} signal
 * @returns {{el: HTMLDetailsElement, open: () => void, stop: () => void}}
 */
export function mountFindings(signal) {
  const list = h("ul", {
    class: "kp-action-list ib-findings-list",
    "aria-label": "Findings of the latest fleet check",
  });
  const notedList = h("ul", {
    class: "kp-action-list ib-findings-noted",
    "aria-label": "Noted findings",
  });
  const notedFold = h(
    "details",
    { class: "ib-findings-noted-fold" },
    h("summary", null, "Noted, not a problem"),
    notedList,
  );
  const split = h("p", { class: "measured ib-findings__split" });
  const body = h("div", { class: "ib-findings" }, split, list, notedFold);
  const card = stage(body);
  const count = h("span", null, "");
  const summary = drivable(
    h(
      "summary",
      { title: "Fold or unfold the findings of the latest fleet check" },
      h("strong", null, "Last nightly check", count),
      h(
        "span",
        { class: "inbox-worth__desc" },
        "Everything the latest fleet check round found: what is broken now and what only drifts, each with what to do. Turn off switches one check off.",
      ),
    ),
    FOLD,
  );
  countOf(count, list, ":scope > li");
  const el = /** @type {HTMLDetailsElement} */ (
    h(
      "details",
      {
        class:
          "kp-card ib-card inbox-worth ib-check-findings kp-accordion__item kp-accordion__item--bare",
        id: "inbox-check-findings",
      },
      summary,
      card,
    )
  );
  let lastSig = "";
  /** @param {any} b */
  const paint = (b) => {
    const v = findingsView(b);
    const sig = JSON.stringify(b?.findings ?? []);
    // The count is the rows drawn in the list (broken and drift); the noted
    // ones are folded and not counted. The split is said in words below.
    count.textContent = ` · ${v.broken.length + v.drift.length}`;
    split.textContent = v.summary;
    const none = !v.broken.length && !v.drift.length && !v.noted.length;
    // refresh-no-replay-1: the same round draws nothing again.
    if (sig !== lastSig) {
      lastSig = sig;
      list.replaceChildren(
        ...[...v.broken, ...v.drift].map((r) => findingRow(r, read)),
      );
      notedList.replaceChildren(...v.noted.map((r) => findingRow(r, read)));
      notedFold.hidden = v.noted.length === 0;
      const label = notedFold.querySelector("summary");
      if (label) label.textContent = `Noted, not a problem · ${v.noted.length}`;
    }
    if (none)
      setCardState(card, "empty", {
        title: "The latest check found nothing",
        text: `${v.passes ? `${v.passes} things passed. ` : ""}Nothing is broken and nothing drifts.`,
      });
    else setCardState(card, "ready");
  };
  const read = () => {
    setCardState(card, "loading", { words: "Reading the fleet check" });
    void slowRead("/data/fleet-check", "the fleet check", signal, paint)
      .then((r) => {
        if (r.ok) paint(r.body);
        else
          setCardState(card, "error", {
            title: "The fleet check could not be read",
            text: `${r.error.what}: ${r.error.why}`,
          });
      })
      .catch(() => {});
  };
  read();
  return {
    el,
    open: () => {
      el.open = true;
      el.scrollIntoView({ block: "start", behavior: "instant" });
    },
    stop: () => {},
  };
}
