// feat-stacks-6: roll back. What the stack can go back to, from
// GET /data/actions/{stack}/rollback-options: the commits that touched its
// files (deploy one of them: deploy-commit) and its native units (put the
// kept previous binary back: rollback-native). What the host cannot tell
// is said plainly. A commit is picked in kp's dated picker and deployed with
// one button; a unit has its own button. Each opens the same action dialog
// as the action's button does.

import { openAction } from "./actiondialog.js";
import { openDialog } from "./actui.js";
import { fetchJson, h, tableBlock, td } from "./dom.js";
import { register } from "./drivehooks.js";
import { rollbackView } from "./rollback.js";
import { stage } from "./stage.js";
import { sortKeys } from "./sortkeys.js";
import {
  attachCardStates,
  setCardState,
  attachDataTables,
  compare,
  attachDatedOptions,
  datedOptions,
  datedValue,
  setDatedOptions,
} from "/static/kp/dist/kp-themes.min.js";

/**
 * @param {string} stack
 */
export async function openRollback(stack) {
  const content = h("div", { class: "rollback" });
  const waiting = stage(content);
  setCardState(waiting, "loading", {
    words: "Reading what this stack can go back to",
  });
  const detachStates = attachCardStates(waiting);
  const commitPicker = h("div", {
    class: "kp-dated",
    "data-kp-dated": "",
    id: "rollback-commit",
    "aria-label": "Commit to deploy",
  });
  const deployChosen = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      "data-commit": "",
    },
    "Deploy the chosen commit",
  );
  const d = openDialog({
    title: `Roll back · ${stack}`,
    description:
      "Go back to an earlier version of the stack's files, or put a native service's previous binary back.",
    body: [waiting],
    id: "rollback-dialog",
    wide: true,
  });
  // feat-platform-10: the Live view replay points at this very list.
  const unregister = register("rollback", {
    dialog: d.dialog,
    closed: d.closed,
    // A commit is chosen in the picker; the one button deploys the choice.
    rowButton: (/** @type {string} */ kind, /** @type {string} */ value) => {
      if (kind !== "unit") {
        datedOptions(commitPicker)?.select(value);
        return deployChosen;
      }
      return /** @type {HTMLElement | null} */ (
        content.querySelector(`button[data-unit="${CSS.escape(value)}"]`)
      );
    },
    close: () => d.close(),
  });
  d.closed.then(unregister);
  d.closed.then(detachStates);
  const r = await fetchJson(
    `/data/actions/${encodeURIComponent(stack)}/rollback-options`,
    "the roll-back options",
  );
  if (!r.ok) {
    setCardState(waiting, "error", {
      title: "Could not read what this stack can go back to",
      text: r.error.why,
      retry: false,
    });
    return;
  }
  const v = rollbackView(r.body);
  const keys = sortKeys();
  deployChosen.addEventListener("click", () => {
    const commit = datedValue(commitPicker);
    if (!commit) return;
    d.close();
    void openAction(stack, "deploy-commit", { preset: { commit } });
  });
  const units = tableBlock({
    remember: "rollback-units",
    caption: "Native units",
    search: "Search units",
    nothing: "This stack declares no native units.",
    columns: [
      { label: "Unit", sort: "text" },
      { label: "Roll back", sort: "text" },
    ],
  });
  units.tbody.append(
    ...v.units.map((u) => {
      const b = h(
        "button",
        { type: "button", class: "kp-button kp-button--sm", "data-unit": u },
        "Put the previous binary back",
      );
      b.addEventListener("click", () => {
        d.close();
        void openAction(stack, "rollback-native", { preset: { unit: u } });
      });
      return h("tr", { "data-kp-row-key": u }, td(u, "mono"), h("td", null, b));
    }),
  );
  content.replaceChildren(
    h("p", null, v.applied),
    ...(v.noWorkingCopy
      ? [
          h(
            "div",
            { class: "kp-alert kp-alert--warning", role: "status" },
            v.noWorkingCopy,
          ),
        ]
      : []),
    h("h3", null, "Deploy an earlier commit"),
    h(
      "p",
      { class: "measured" },
      `Commits that touched stacks/${stack}, the newest first. Pick the moment to go back to, then deploy it.`,
    ),
    commitPicker,
    h("div", { class: "kp-dialog__actions" }, deployChosen),
    h("h3", null, "Native services"),
    ...(v.units.length
      ? [units.wrap]
      : [
          h("p", { class: "measured" }, "This stack declares no native units."),
        ]),
    ...(v.missing.length
      ? [
          h(
            "div",
            { class: "kp-alert kp-alert--info missing", role: "note" },
            h("strong", null, "What the host cannot tell (yet)"),
            h("ul", null, ...v.missing.map((m) => h("li", null, m))),
          ),
        ]
      : []),
  );
  const detach = attachDataTables(content, { compare: keys.compare(compare) });
  const detachDated = attachDatedOptions(content);
  setDatedOptions(
    commitPicker,
    v.commits.map((c) => ({
      value: c.commit,
      label: c.subject,
      when: c.at * 1000,
      meta: c.applied ? `${c.short} · deployed now` : c.short,
    })),
  );
  if (!v.commits.length)
    datedOptions(commitPicker)?.setState("empty", {
      reason: v.noWorkingCopy
        ? "No working copy yet"
        : "No commits to go back to",
    });
  deployChosen.hidden = !v.commits.length;
  setCardState(waiting, "ready");
  d.closed.then(() => {
    detach();
    detachDated();
  });
}
