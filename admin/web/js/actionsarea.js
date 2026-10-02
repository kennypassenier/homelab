// feat-stacks-4: the actions area on a stack's page and on the host page,
// drawn from the catalog. Each button opens the action's dialog; a job
// running on this target shows its live panel underneath (feat-ops-6).

import { act, catalogReady, onAct } from "./act.js";
import { openAction } from "./actiondialog.js";
import { hostActions, stackActionGroups } from "./actionforms.js";
import { h, sectionHeader } from "./dom.js";
import { finished } from "./jobs.js";
import { current } from "./store.js";
import { mountJobPanel } from "./jobpanel.js";
import { openRollback } from "./rollbackdialog.js";

/** fix-210: one short, plain sentence per action group, so a button's
 * purpose never has to be guessed from its group's bare name. A group
 * missing here (a future catalog addition) still gets a generic fallback
 * rather than no description at all. */
const GROUP_DESC = /** @type {Record<string, string>} */ ({
  "Deploy and update": "Apply this stack's files, or check for an update.",
  Backups: "Back this stack up now, or restore it from a snapshot.",
  Secrets: "Push or rotate what this stack reads from latch.",
  Parking: "Pause or resume this stack without destroying it.",
  "Native services":
    "Install, update or roll back a native (non-docker) service.",
  Retire: "Destroy this stack, or hand it off to another orchestrator.",
});

/**
 * @param {{stack: string} | {host: true}} target
 * @returns {{element: HTMLElement, stop: () => void}}
 */
export function mountActionsArea(target) {
  const isHost = "host" in target;
  const buttons = h(
    "div",
    { class: "actions-groups" },
    h("p", { class: "measured" }, "Reading the actions…"),
  );
  const runningBox = h("div", { class: "actions-running" });
  const element = h(
    "section",
    {
      class: "kp-card actions-area",
      "aria-label": isHost ? "Host-wide actions" : "Actions",
      id: "actions",
    },
    sectionHeader(
      isHost ? "Host-wide actions" : "Actions",
      isHost
        ? "Actions that act on the host itself, not on any one stack."
        : "Every action you can run on this one stack, grouped by what it does.",
    ),
    buttons,
    runningBox,
  );
  let stopped = false;
  /** @type {Map<number, () => void>} */
  const panels = new Map();

  /** @param {import("./actionforms.js").CatalogEntry} entry @param {string | null} refused @param {string} stack */
  const button = (entry, refused, stack) => {
    const b = h(
      "button",
      {
        type: "button",
        class: `kp-button${entry.scope === "all" ? " kp-button--destructive" : ""}`,
        "data-action": entry.action,
        title: refused ? `${entry.what} (${refused})` : entry.what,
      },
      entry.label,
    );
    if (refused) b.disabled = true;
    b.addEventListener(
      "click",
      () => void openAction(stack, entry.action, { openRollback }),
    );
    return b;
  };

  void catalogReady().then((catalog) => {
    if (stopped) return;
    if (!catalog) {
      buttons.replaceChildren(
        h(
          "p",
          { class: "kp-alert kp-alert--destructive", role: "alert" },
          "The dashboard did not send its action catalog; reload the page.",
        ),
      );
      return;
    }
    if (isHost) {
      buttons.replaceChildren(
        h(
          "div",
          { class: "actions-row" },
          ...hostActions(catalog).map((e) =>
            button(e, null, catalog.host_target),
          ),
        ),
      );
      return;
    }
    const stack = target.stack;
    const rb = h(
      "button",
      { type: "button", class: "kp-button", "data-action": "rollback" },
      "Roll back…",
    );
    rb.addEventListener("click", () => void openRollback(stack));
    buttons.replaceChildren(
      ...stackActionGroups(
        catalog,
        stack,
        // fix-229: what the host says about this stack; `null` (an older
        // host, or the fleet not read yet) offers every action as before.
        current().fleet?.stacks?.find((s) => s.name === stack)?.native ?? null,
      ).map((g) =>
        h(
          "div",
          { class: "actions-group" },
          (() => {
            const head = sectionHeader(
              g.group,
              GROUP_DESC[g.group] ?? "Actions on this stack.",
              { level: "h3" },
            );
            head.classList.add("actions-group__label");
            return head;
          })(),
          h(
            "div",
            { class: "actions-row" },
            ...g.actions.map((a) => button(a.entry, a.refused, stack)),
            ...(g.group === "Deploy and update" ? [rb] : []),
          ),
        ),
      ),
    );
  });

  const name = isHost ? "_host" : target.stack;
  const paintRunning = () => {
    const live = act.jobs.filter((j) => j.stack === name && !finished(j.state));
    // A job that finished stays until the page is left, so its end is read.
    const keep = new Set([...live.map((j) => j.job), ...panels.keys()]);
    for (const id of keep)
      if (!panels.has(id)) {
        const p = mountJobPanel(id);
        panels.set(id, p.stop);
        runningBox.prepend(p.element);
      }
  };
  const off = onAct("jobs", paintRunning);
  paintRunning();
  return {
    element,
    stop: () => {
      stopped = true;
      off();
      for (const s of panels.values()) s();
    },
  };
}
