// Resolving commits the working copy holds and the remote lacks: push,
// rebase onto the remote and push, or drop them (asked for twice).

import { send } from "../../act.js";
import { notify, refusalAlarm } from "../../actui.js";
import { drivable } from "../../drivable.js";
import { h } from "../../dom.js";
import { REPO_CHOICE } from "./controls.js";

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
export function resolveButton(choice, reload) {
  const b = h(
    "button",
    {
      type: "button",
      class: `kp-button kp-button--sm${choice === "drop" ? " kp-button--destructive" : ""}`,
      "data-choice": choice,
      title: {
        push: "Push the commits to the remote",
        rebase: "Put the commits on top of the remote's, then push",
        drop: "Throw the commits away; the working copy goes back to the remote",
      }[choice],
    },
    { push: "Push", rebase: "Rebase and push", drop: "Drop them" }[choice],
  );
  drivable(b, REPO_CHOICE, choice);
  b.addEventListener("click", async () => {
    if (choice === "drop") {
      const { openConfirmation } =
        await import("/static/kp/dist/kp-themes.min.js");
      openConfirmation(b, {
        phrase: "Drop the unpushed commits?",
        typeName: "drop",
        loses: [
          "The working copy goes back to the remote's branch.",
          "Those commits are gone from here.",
        ],
        acceptLabel: "Drop the commits",
        onAccept: () => void resolve(choice, reload),
      });
      return;
    }
    await resolve(choice, reload);
  });
  return b;
}

/**
 * @param {"push" | "rebase" | "drop"} choice
 * @param {() => Promise<void>} reload
 */
async function resolve(choice, reload) {
  const r = await send(
    "POST",
    "/data/repo/unpushed",
    choice === "drop" ? { choice, confirm: "drop" } : { choice },
    "the unpushed commits",
  );
  if (!r.ok) await refusalAlarm(r.error, r.status);
  else notify(choice === "drop" ? "Dropped." : "Pushed.", "success");
  void reload();
}
