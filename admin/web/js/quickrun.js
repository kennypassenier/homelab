// ux-stackhub-4: an action that loses nothing runs at once. Back up is the
// case: its dialog said "Nothing is lost or changed" and still asked for a
// confirm. This sends the same request the dialog's last press sends (same
// arguments, same route, so the same refusals), tells the person it started
// in a toast, and tells the result in a second one that opens Activity.
// The job itself shows on the stack's Overview (its live panel) and in
// Activity, which is where the result stays after the toast is gone.

import { act, catalogReady, onAct, send } from "./act.js";
import { actionForm, buildArgs, initialValues } from "./actionforms.js";
import { notify, refusalAlarm } from "./actui.js";
import { declare } from "./drivable.js";
import { finished } from "./jobs.js";
import { toast } from "./ui.js";

const SEE_ACTIVITY = declare({
  id: "quickrun-see-activity",
  page: "stack",
  opens: "view",
  what: "the toast of a backup that started or ended: open Activity at the running jobs",
  shows: "after Back up is pressed in a stack's header",
});

/** Where Activity lists the jobs that run and have run. */
const ACTIVITY = "/activity?view=running";

/** @param {string} href */
function open(href) {
  history.pushState(null, "", href);
  dispatchEvent(new PopStateEvent("popstate"));
}

/** @param {string} action the catalog action, e.g. "backup" */
const verbs = (action) =>
  action.startsWith("backup")
    ? { doing: "Backing up", done: "backed up" }
    : { doing: "Running", done: "done" };

/**
 * The toast for the end of a job: what happened, in one sentence.
 * @param {string} stack
 * @param {string} action
 * @param {import("./jobs.js").Job} job
 */
function tellEnd(stack, action, job) {
  const v = verbs(action);
  if (job.state === "done") {
    toast(`${stack} ${v.done}. The run is kept in Activity.`, {
      action: {
        label: "See in Activity",
        run: () => open(ACTIVITY),
        drive: { id: SEE_ACTIVITY },
      },
      replace: "all",
    });
    return;
  }
  const why = job.message ? `: ${job.message}` : "";
  notify(`${v.doing} ${stack} did not finish (${job.state})${why}.`, "error", {
    label: "See in Activity",
    onClick: () => open(ACTIVITY),
  });
}

/**
 * Follow one job to its end, then tell it. The subscription is the store's,
 * so it outlives the page the press was made on.
 * @param {number} id
 * @param {(job: import("./jobs.js").Job) => void} then
 */
function follow(id, then) {
  let ended = false;
  const check = () => {
    const job = act.jobs.find((j) => j.job === id);
    if (ended || !job || !finished(job.state)) return;
    ended = true;
    stop();
    then(job);
  };
  const stop = onAct("jobs", check);
  check();
}

/**
 * Run a catalog action on a stack now, without a confirm.
 * @param {string} stack
 * @param {string} action an action that loses nothing (backup, backup-native)
 * @returns {Promise<boolean>} whether the job was accepted
 */
export async function runNow(stack, action) {
  const catalog = await catalogReady();
  const entry = catalog?.actions.find((a) => a.action === action);
  if (!catalog || !entry) {
    notify(`The dashboard does not know the action "${action}".`, "error");
    return false;
  }
  const form = actionForm(entry, {
    stack,
    selfStack: catalog.self_stack,
    hostTarget: catalog.host_target,
  });
  if (form.refused) {
    await refusalAlarm({ what: form.title, why: form.refused, fix: "" }, 403);
    return false;
  }
  const r = await send(
    "POST",
    form.runPath,
    buildArgs(form, initialValues(form)),
    form.title,
  );
  if (!r.ok) {
    await refusalAlarm(r.error, r.status);
    return false;
  }
  const id = /** @type {number} */ (r.body.job);
  const started = toast(`${verbs(action).doing} ${stack}…`, {
    action: {
      label: "Follow",
      run: () => open(ACTIVITY),
      drive: { id: SEE_ACTIVITY },
    },
  });
  follow(id, (job) => {
    started();
    tellEnd(stack, action, job);
  });
  return true;
}
