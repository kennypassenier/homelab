// The Host page's Host actions card: what the dashboard can do to the host
// itself, grouped by intent, from the action catalog; the jobs that are
// running on the host show as panels under the tiles.

import { act, catalogReady, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { h } from "../dom.js";
import { actionBlurb, hostActionGroups } from "../host.js";
import { finished } from "../jobs.js";
import { mountJobPanel } from "../jobpanel.js";
import { openRollback } from "../rollbackdialog.js";
import { section } from "../ui.js";

/** @param {import("./hostenv.js").HostEnv} env */
export function createActions(env) {
  const { abort, stops } = env;
  const groupsBox = h(
    "div",
    { class: "hk-act-groups", id: "host-actions" },
    h("p", { class: "hk-muted", role: "status" }, "Reading the actions…"),
  );
  const runningBox = h("div", { class: "hk-running" });
  const actionsCard = section({
    id: "host-actions-card",
    title: "Host actions",
    desc: "What the dashboard can do to the host itself; each opens a dialog that says what will happen first.",
    foot: [
      "Every action becomes a job you can follow on Activity",
      h("a", { class: "hk-link", href: "/activity" }, "Open Activity"),
    ],
  });
  actionsCard.el.classList.add("actions-area");
  actionsCard.body.append(groupsBox, runningBox);
  void catalogReady().then((catalog) => {
    if (abort.signal.aborted) return;
    if (!catalog) {
      groupsBox.replaceChildren(
        h(
          "p",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          "The dashboard did not send its action catalog; reload the page.",
        ),
      );
      return;
    }
    const entries = catalog.actions.filter((a) => a.target === "host");
    groupsBox.replaceChildren(
      ...hostActionGroups(entries).map((g) =>
        h(
          "div",
          { class: "hk-act-group", "data-group": g.title },
          h(
            "h3",
            null,
            g.title,
            g.full
              ? h(
                  "span",
                  { class: "hk-lock", title: "Needs the full-access token" },
                  "full access",
                )
              : null,
          ),
          g.actions.map((a) =>
            h(
              "button",
              {
                type: "button",
                class: `hk-act${a.destructive ? " hk-act--destructive" : ""}`,
                "data-action": a.action,
                title: `${a.label}: ${actionBlurb(a.what)} Opens a dialog that says what will happen, then starts it.`,
                onclick: () =>
                  void openAction(catalog.host_target, a.action, {
                    openRollback,
                  }),
              },
              h("b", null, a.label),
              h("span", null, actionBlurb(a.what)),
              h("em", { "aria-hidden": "true" }, "›"),
            ),
          ),
        ),
      ),
    );
  });
  /** @type {Map<number, () => void>} */
  const panels = new Map();
  const paintRunning = () => {
    const live = act.jobs.filter(
      (j) => j.stack === "_host" && !finished(j.state),
    );
    for (const j of live)
      if (!panels.has(j.job)) {
        const p = mountJobPanel(j.job, { framed: true });
        panels.set(j.job, p.stop);
        runningBox.prepend(p.element);
      }
  };
  stops.push(onAct("jobs", paintRunning));
  paintRunning();
  stops.push(() => {
    for (const s of panels.values()) s();
  });
  return { card: actionsCard };
}
