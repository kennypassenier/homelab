// The one review every New stack route ends on (ux-stacknew-2, Kenny
// 2026-11-01): the plan (what will be created, which address it gets), the
// commit fields (subject, note, whether to deploy next) and the commit
// itself. A preset, an empty stack and a bundle differ only in the URLs and
// the bodies they hand in; the wizard that shows this part lives on a page
// of its own (pages/newstack.js), like Restore.

import { send } from "./act.js";
import { notify, refusalAlarm, refusalCallout } from "./actui.js";
import { driven } from "./drivehooks.js";
import { h } from "./dom.js";
import { commitFields, startValues } from "./editforms.js";
import { editField, markErrors, planBlock } from "./editui.js";
import { mountJobPanel } from "./jobpanel.js";
import { committedText } from "./plan.js";
import { stackHref } from "./router.js";

/**
 * @typedef {{
 *   what: string,
 *   planUrl: string,
 *   planBody: () => unknown,
 *   commitUrl: string,
 *   commitBody: (v: Record<string, string | boolean>, plan: import("./plan.js").Plan) => unknown,
 *   stackName: () => string,
 *   canCommit: (ok: boolean) => void,
 *   onCommitted: (answer: any) => void,
 * }} ReviewSpec
 */

/**
 * The review step's content and its two verbs, load and commit.
 * @param {ReviewSpec} o
 */
export function reviewPart(o) {
  const planBox = h("div", { class: "plan-box", "aria-live": "polite" });
  const commitBox = h("div", { class: "commit-box" });
  const runError = h("div");
  /** @type {import("./plan.js").Plan | null} */
  let plan = null;
  /** @type {Record<string, string | boolean>} */
  let values = {};
  /** @type {Map<string, HTMLElement>} */
  const inputs = new Map();
  let running = false;

  const load = async () => {
    plan = null;
    o.canCommit(false);
    planBox.replaceChildren(h("p", { class: "measured" }, "Making the plan…"));
    commitBox.replaceChildren();
    const x = await send("POST", o.planUrl, o.planBody(), o.what);
    if (!x.ok) {
      planBox.replaceChildren(
        refusalCallout(x.error, "destructive", "No plan"),
      );
      return;
    }
    plan = /** @type {import("./plan.js").Plan} */ (x.body);
    planBox.replaceChildren(planBlock(plan));
    const fields = commitFields(plan.follow_ups, plan.subject);
    values = startValues({
      steps: [{ id: "commit", label: "Commit", fields }],
    });
    inputs.clear();
    commitBox.replaceChildren(
      ...fields.map((f) => {
        const y = editField(f, values[f.name], (v) => (values[f.name] = v));
        inputs.set(f.name, y.input);
        return y.wrap;
      }),
    );
    o.canCommit(plan.valid);
  };

  const commit = async () => {
    // Live view: the commit runs on the dashboard's server, once.
    if (running || !plan?.valid || driven()) return;
    if (!String(values.subject ?? "").trim()) {
      markErrors(inputs, { subject: "A commit needs a subject." });
      return;
    }
    running = true;
    o.canCommit(false);
    runError.replaceChildren(
      h("p", { class: "measured" }, "Committing and pushing…"),
    );
    const x = await send(
      "POST",
      o.commitUrl,
      o.commitBody(values, plan),
      o.what,
    );
    running = false;
    o.canCommit(true);
    if (!x.ok) {
      runError.replaceChildren(refusalCallout(x.error));
      await refusalAlarm(x.error, x.status);
      return;
    }
    o.onCommitted(x.body);
  };

  return {
    els: [planBox, commitBox, runError],
    load,
    commit,
    planReady: () => plan !== null,
    /** @param {import("./doctor.js").RouteError | null} e */
    runError: (e) =>
      runError.replaceChildren(...(e ? [refusalCallout(e)] : [])),
  };
}

/**
 * What the commit answered, in the wizard's place: the words, the follow-up
 * deploy's job when one was queued, then Open <stack> and Back to Stacks.
 * @param {any} answer
 * @param {string} name the new stack
 * @param {(href: string) => void} navigate
 * @returns {{nodes: Node[], stop: () => void}}
 */
export function committedView(answer, name, navigate) {
  const text = committedText(answer);
  notify(text, "success");
  /** @type {Node[]} */
  const nodes = [
    h("div", { class: "kp-alert kp-alert--success", role: "status" }, text),
  ];
  let stop = () => {};
  if (answer.follow?.refused)
    nodes.push(refusalCallout(answer.follow.refused, "warning", "Not queued"));
  if (typeof answer.follow?.job === "number") {
    const panel = mountJobPanel(answer.follow.job, { compact: true });
    stop = panel.stop;
    nodes.push(panel.element);
  }
  /** @param {string} href @param {string} label @param {string} cls */
  const link = (href, label, cls) => {
    const a = h("a", { class: cls, href }, label);
    a.addEventListener("click", (ev) => {
      ev.preventDefault();
      navigate(href);
    });
    return a;
  };
  nodes.push(
    h(
      "div",
      { class: "kp-wizard__actions" },
      link("/stacks", "Back to Stacks", "kp-button"),
      link(
        stackHref(name, "settings"),
        `Open ${name}`,
        "kp-button kp-button--primary",
      ),
    ),
  );
  return { nodes, stop };
}
