// feat-shell-4 (redesign 3.71.0, decision "Inbox", Kenny approved
// 2026-10-03): everything waiting for a person, worst first — the host's
// questions, then every row of inbox.js's list (the same list the bar's
// counter counts, so the number there is the number of rows here), then
// what Health held (Today, Doctor, Checks) as folded "worth a look"
// blocks, `?kind=` opening one. This is the foundation's Inbox: the Inbox
// page helper redraws the rows to the approved demo (flows/inbox.html).

import { mountAsks } from "../asksui.js";
import { h } from "../dom.js";
import { inboxNow, onInbox } from "../inbox.js";
import { mountHealthBlocks } from "./health.js";
import { emptyState, pageHeader, section, skeletonLines } from "../ui.js";

/** @param {import("../inbox.js").Severity} s */
const sevWord = (s) =>
  s === "bad" ? "urgent" : s === "warn" ? "warning" : "worth a look";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const head = pageHeader({
    title: "Inbox",
    desc: "Everything waiting for a person, worst first: the host's questions, failures, updates and checks, each with what to do about it.",
    live: "updated",
  });
  const asks = h("div", { class: "asks inbox-asks", hidden: "" });
  const list = h("ul", {
    class: "nx-inbox",
    "aria-label": "Waiting for you",
  });
  const rows = section({
    title: "Waiting for you",
    desc: "One row per thing that needs a person; the counter in the bar is the number of rows here.",
  });
  rows.body.append(list);
  const blocks = h("div", { class: "health-blocks" });
  const worth = section({
    title: "Worth a look",
    desc: "Today's reading, the host's own doctor and the checks only a person can answer; each opens on its own.",
  });
  worth.body.append(blocks);
  root.replaceChildren(head.el, asks, rows.el, worth.el);

  const paint = () => {
    const { items, ready } = inboxNow();
    if (!ready) {
      list.replaceChildren(
        h("li", null, skeletonLines(3, "Reading the Inbox")),
      );
      return;
    }
    head.live?.set(Date.now() / 1000);
    if (items.length === 0) {
      list.replaceChildren(
        h(
          "li",
          { class: "nx-inbox__empty" },
          emptyState({
            title: "Nothing needs you",
            text: "The Inbox watches the host's questions, failed jobs, alerts and the nightly check; when one needs a person it shows here, and the counter in the bar says how many.",
          }),
        ),
      );
      return;
    }
    list.replaceChildren(
      ...items
        .filter((i) => i.source !== "asks")
        .map((i) =>
          h(
            "li",
            {
              class: "nx-inbox__row",
              "data-severity": i.severity,
              id: i.key.replace(/[^a-z0-9-]/gi, "-"),
            },
            h("span", {
              class: `nx-sev nx-sev--${i.severity}`,
              title: sevWord(i.severity),
            }),
            h(
              "div",
              { class: "nx-inbox__what" },
              h("strong", null, i.title),
              ...(i.why ? [h("span", null, i.why)] : []),
            ),
            h(
              "div",
              { class: "nx-inbox__acts" },
              h(
                "a",
                { class: "kp-button kp-button--sm", href: i.href },
                "Open",
              ),
            ),
          ),
        ),
    );
  };
  const offAsks = mountAsks(asks);
  const off = onInbox(paint);
  paint();
  const stopBlocks = mountHealthBlocks(
    blocks,
    new URLSearchParams(location.search).get("kind"),
  );
  return () => {
    off();
    offAsks();
    stopBlocks();
    rows.stop();
    worth.stop();
  };
}
