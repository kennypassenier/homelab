// feat-stacks-6: roll back. What the stack can go back to, from
// GET /data/actions/{stack}/rollback-options: the commits that touched its
// files (deploy one of them: deploy-commit) and its native units (put the
// kept previous binary back: rollback-native). What the host cannot tell
// is said plainly. Each choice opens the same action dialog as a button.

import { openAction } from "./actiondialog.js";
import { openDialog, refusalCallout } from "./actui.js";
import { fetchJson, h, tableBlock, td } from "./dom.js";
import { register } from "./drivehooks.js";
import { formatDateTime } from "./format.js";
import { rollbackView } from "./rollback.js";
import { sortKeys } from "./sortkeys.js";
import {
  attachDataTables,
  compare,
  dataTable,
} from "/static/kp/js/datatable.js";

/**
 * @param {string} stack
 */
export async function openRollback(stack) {
  const status = h(
    "p",
    { class: "measured" },
    "Reading what this stack can go back to…",
  );
  const content = h("div", { class: "rollback" }, status);
  const d = openDialog({
    title: `Roll back · ${stack}`,
    description:
      "Go back to an earlier version of the stack's files, or put a native service's previous binary back.",
    body: [content],
    id: "rollback-dialog",
    wide: true,
  });
  // feat-platform-10: the Live view replay points at this very list.
  const unregister = register("rollback", {
    dialog: d.dialog,
    closed: d.closed,
    rowButton: (/** @type {string} */ kind, /** @type {string} */ value) =>
      /** @type {HTMLElement | null} */ (
        content.querySelector(
          `button[data-${kind === "unit" ? "unit" : "commit"}="${CSS.escape(value)}"]`,
        )
      ),
    close: () => d.close(),
  });
  d.closed.then(unregister);
  const r = await fetchJson(
    `/data/actions/${encodeURIComponent(stack)}/rollback-options`,
    "the roll-back options",
  );
  if (!r.ok) {
    content.replaceChildren(
      refusalCallout(r.error, "destructive", "Could not read"),
    );
    return;
  }
  const v = rollbackView(r.body);
  const keys = sortKeys();
  const commits = tableBlock({
    remember: "rollback-commits",
    caption: `Commits that touched stacks/${stack}`,
    search: "Search commits",
    nothing: "No commits to go back to.",
    columns: [
      { label: "When", sort: "time" },
      { label: "Commit", sort: "text" },
      { label: "Subject", sort: "text", cls: "wide" },
      { label: "Now", sort: "text", filter: "choice" },
      { label: "Deploy", sort: "text" },
    ],
  });
  commits.tbody.append(
    ...v.commits.map((c) => {
      const b = h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--sm",
          "data-commit": c.commit,
        },
        "Deploy this",
      );
      b.addEventListener("click", () => {
        d.close();
        void openAction(stack, "deploy-commit", {
          preset: { commit: c.commit },
        });
      });
      return h(
        "tr",
        { "data-kp-row-key": c.commit },
        td(keys.note("time", formatDateTime(c.at), c.at)),
        td(c.short, "mono"),
        td(c.subject),
        td(c.applied),
        h("td", null, b),
      );
    }),
  );
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
    ...(v.commits.length
      ? [commits.wrap]
      : [h("p", { class: "measured" }, "No commits to go back to.")]),
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
  dataTable(commits.wrap)?.refresh();
  d.closed.then(detach);
}
