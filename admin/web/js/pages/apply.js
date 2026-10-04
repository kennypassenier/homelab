// Deploy all changes (TUI parity, ask-8's `homelab apply`, dash-apply:
// "Yes, plan first"; renamed from "Apply the whole fleet" in 3.71.0): every
// stack of the working copy against what the host last applied.
//
// redesign-flows-4 (redesign 3.71.0, demo apply.html; Kenny approved
// 2026-10-03): the plan as three steps (Compare → Review the plan → Apply),
// four count tiles that each show only their column (a plain click, again
// shows all), then three columns — Will deploy (untick one to leave it
// out; its diff on request), Will be destroyed (a stack whose directory is
// gone is included only when its name is typed) and Cannot be planned
// (why, the fix, and its stack file) — and one bar that says what Apply
// will do before it is pressed.
//
// redesign-flows-5 (the senior review, 2026-10-03, and the coordinator's
// destroy rule): one heading "Deploy all changes" (the caller's) with one
// sentence, then the steps — Compare (done only once the compare ran),
// Review the plan, Apply, and Destroy as its own red step. Apply deploys
// the ticked stacks (`leave_out` names the unticked ones and those that
// cannot be planned); a destroy never rides along with a deploy: it is
// armed by typing the stack's name, confirmed by its own tick, sent with the
// CT number the plan showed, and only once no ticked deploy is waiting.
// Pure half: js/applyplan.js.
//
// `mount` paints only the section's body (the caller — Stacks' "Deploy all
// changes" — owns its heading and one-line description) and starts with a
// skeleton in the final geometry before the plan has been read.

import { act, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { destroyStep, goPlan } from "../applyplan.js";
import { ensureStyle, errorBox, fetchJson, h } from "../dom.js";
import {
  declare,
  declareField,
  drivable,
  fieldId,
  viaForm,
} from "../drivable.js";
import { diffBlocks } from "../editui.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";
import { current } from "../store.js";
import { stackMark } from "../ui.js";

const READ = declare({
  id: "deploy-all-plan-again",
  page: "overview",
  opens: "run",
  what: "read the plan of every stack again from the host and the working copy",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const TILE = declare({
  id: "deploy-all-tile",
  page: "overview",
  opens: "view",
  row: "deploy|destroy|broken|same",
  what: "show only one column of the plan (a plain click; again shows all)",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const PICK = declare({
  id: "deploy-all-pick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "include or leave out one stack of the deploy batch",
  selects: true,
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const DIFF = declare({
  id: "deploy-all-diff",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "show or hide what deploying one stack changes",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
// redesign-integrate-8: a text box is a field Live view types into, never
// a control it clicks (a click on it does nothing; the sweep said so).
const TYPE = declareField({
  id: "deploy-all-destroy-name",
  page: "overview",
  row: "<stack>",
  what: "type a gone stack's name to include its destroy",
});
const FILE = declare({
  id: "deploy-all-stack-file",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open the stack file of a stack that cannot be planned (its Settings)",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const SHOW_ALL = declare({
  id: "deploy-all-show-all",
  page: "overview",
  opens: "view",
  what: "show every column of the plan again (Esc does the same)",
  shows: "in the Deploy all changes panel, while one column is shown",
  reach: [
    { do: "click", control: "stacks-deploy-all" },
    { do: "click", control: "deploy-all-tile", row: "*" },
  ],
});
const DESTROY_ACK = declare({
  id: "deploy-all-destroy-confirm",
  page: "overview",
  opens: "view",
  what: "tick the destroy step's own red confirmation",
  shows: "in the Deploy all changes panel, once a gone stack's name is typed",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
// review M5: every page field Live view may set is declared (drivable.js
// `declareField`); the client and the dashboard refuse any other.
const DESTROY_ACK_FIELD = declareField({
  id: "apply-destroy-ack",
  page: "overview",
  what: "the destroy step's own red confirmation (a tick)",
});
const DESTROY = declare({
  id: "deploy-all-destroy",
  page: "overview",
  opens: "dialog",
  what: "open the destroy of the armed gone stacks, a separate confirmed step after the deploys",
  shows:
    "in the Deploy all changes panel, once a destroy is armed and confirmed",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});

/**
 * @typedef {import("../applyplan.js").Plan} Plan
 * @typedef {"deploy" | "destroy" | "broken" | "same"} Col
 * @typedef {{button: HTMLElement, at: () => number | null,
 *   onChange: (f: () => void) => () => void}} Compare
 */

export { goPlan } from "../applyplan.js";

/** The skeleton in the final geometry: tiles, three columns, the bar. */
function skeleton() {
  return h(
    "div",
    { class: "ap-body apply-grid--skeleton", "aria-hidden": "true" },
    h(
      "div",
      { class: "ap-counts" },
      ...Array.from({ length: 4 }, () =>
        h("div", { class: "nx-kpi apply-row apply-row--skeleton" }),
      ),
    ),
    h(
      "div",
      { class: "ap-cols" },
      ...Array.from({ length: 3 }, () =>
        h("div", { class: "ap-item apply-row apply-row--skeleton" }),
      ),
    ),
  );
}

/**
 * @param {HTMLElement} root
 * @param {{compare?: Compare, onRead?: (read: {pending: number,
 *   broken: number}) => void}} [opts] `compare`: the Compare step's own
 *   button and when it last ran (Stacks' "Compare again"); `onRead`: the
 *   plan's own count once read, for the Stacks header's button
 *   (redesign-stacks, merged with redesign-flows-4 in 3.71.0)
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  ensureStyle("/css/pages/apply.css");
  // redesign-final-c1: the plan lays out by its own width (apply.css).
  root.classList.add("ap-root");
  const read = drivable(
    h(
      "button",
      {
        type: "button",
        class: "kp-button",
        id: "apply-read",
        title: "Read the plan again from the host and the working copy",
      },
      "Plan again",
    ),
    READ,
  );
  const ago = agoEl("planned");
  const compared = agoEl("compared");
  /** @param {string} key @param {string} title @param {string} what @param {...Node} more */
  const stepLi = (key, title, what, ...more) =>
    h(
      "li",
      { "data-step": key },
      h("strong", null, title),
      h("span", null, what),
      ...(more.length ? [h("div", { class: "ap-steps__more" }, ...more)] : []),
    );
  const steps = h("ol", { class: "ap-steps", "aria-label": "Steps" });
  const err = h("div");
  const body = h("div", { class: "ap-body", id: "apply-body" });
  const go = h("div", {
    class: "ap-go",
    role: "region",
    "aria-label": "Apply",
    id: "apply-head",
    "aria-live": "polite",
  });
  const destroyBox = h("section", {
    class: "ap-destroy",
    id: "apply-destroy-step",
    "aria-label": "Destroy",
    hidden: "",
  });
  root.replaceChildren(
    steps,
    h(
      "div",
      { class: "ap-top" },
      h("div", { class: "ap-top__right" }, ago, read),
    ),
    err,
    body,
    go,
    destroyBox,
  );
  body.replaceChildren(skeleton());
  const abort = new AbortController();

  /** @type {Plan | null} */
  let plan = null;
  /** @type {Set<string>} */
  let ticked = new Set();
  /** @type {Map<string, string>} */
  const typedText = new Map();
  /** @type {Col | null} */
  let only = null;
  /** @type {string | null} */
  let open = null;
  /** @type {Map<string, Node[]>} */
  const diffs = new Map();
  let ack = false;
  /** @type {"none" | "pressed" | "running" | "done"} */
  let applied = "none";
  /** @type {number | null} unix seconds of this page's Apply press */
  let pressedAt = null;

  const typed = () =>
    new Set((plan?.destroy ?? []).filter((s) => typedText.get(s) === s));

  const vmid = (/** @type {string} */ s) =>
    current().fleet?.stacks.find((x) => x.name === s)?.vmid ?? null;

  const paintSteps = () => {
    const at = opts.compare?.at() ?? null;
    setAgo(compared, at);
    const list = [
      stepLi(
        "compare",
        "Compare",
        at == null
          ? "Each stack against its files; not compared yet"
          : "Each stack against its files",
        ...(opts.compare
          ? [opts.compare.button, ...(at != null ? [compared] : [])]
          : []),
      ),
      stepLi(
        "review",
        "Review the plan",
        "What applying would change; nothing runs yet",
      ),
      stepLi(
        "apply",
        "Apply",
        "The ticked deploys, one confirmed batch, each backed up first",
      ),
      ...(plan?.destroy.length
        ? [
            stepLi(
              "destroy",
              "Destroy",
              "Its own red step after the deploys: typed names, one more confirmation",
            ),
          ]
        : []),
    ];
    const mark = (/** @type {string} */ k, /** @type {string} */ cls) =>
      list.find((l) => l.dataset.step === k)?.classList.add(cls);
    if (at != null) mark("compare", "done");
    if (applied === "done") mark("apply", "done");
    if (plan) {
      if (applied === "pressed" || applied === "running") mark("apply", "on");
      else mark("review", "on");
      if (plan.destroy.length && typed().size) mark("destroy", "armed");
    } else mark("compare", "on");
    for (const l of list)
      if (l.classList.contains("on")) l.setAttribute("aria-current", "step");
    steps.style.setProperty("--ap-steps", String(list.length));
    steps.replaceChildren(...list);
  };
  const stopCompare = opts.compare?.onChange(paintSteps) ?? (() => {});

  /** @param {string} s */
  const loadDiff = async (s) => {
    diffs.set(s, [h("p", { class: "ap-hint" }, "Reading…")]);
    paint();
    const r = await fetchJson(
      `/data/plan/${encodeURIComponent(s)}`,
      `the plan of ${s}`,
      abort.signal,
    );
    if (abort.signal.aborted) return;
    if (!r.ok) diffs.set(s, [errorBox(r.error)]);
    else {
      const files = fileViews(r.body.files ?? []);
      diffs.set(s, [
        ...(r.body.note ? [h("p", { class: "ap-hint" }, r.body.note)] : []),
        ...(files.length
          ? diffBlocks(files)
          : [h("p", null, "No file changes.")]),
      ]);
    }
    paint();
  };

  const paint = () => {
    paintSteps();
    const p = plan;
    if (!p) return;
    /** @param {Col} key @param {string} label @param {number} n @param {string} ctx @param {string} [tone] */
    const tile = (key, label, n, ctx, tone) => {
      const b = drivable(
        h(
          "button",
          {
            type: "button",
            class: `nx-kpi ap-tile${tone ? ` ap-tile--${tone}` : ""}`,
            "aria-pressed": String(only === key),
            title:
              "Click to show only this column; again, Show all or Esc shows all",
          },
          h("span", { class: "nx-kpi__label" }, label),
          h("span", { class: "nx-kpi__value" }, String(n)),
          h("span", { class: "nx-kpi__ctx" }, ctx),
        ),
        TILE,
        key,
      );
      b.addEventListener("click", () => {
        only = only === key ? null : key;
        paint();
      });
      return b;
    };
    const show = (/** @type {Col} */ k) => !only || only === k;
    const col = (
      /** @type {Col} */ key,
      /** @type {string} */ title,
      /** @type {string} */ dot,
      /** @type {number} */ n,
      /** @type {string} */ desc,
      /** @type {Node[]} */ items,
    ) =>
      h(
        "section",
        {
          class: "ap-col",
          ...(show(key) ? {} : { hidden: "" }),
          "aria-label": title,
          "data-col": key,
        },
        h(
          "h3",
          null,
          h("span", { class: `nx-sev nx-sev--${dot}`, "aria-hidden": "true" }),
          title,
          h("span", { class: "kp-badge" }, String(n)),
        ),
        h("p", { class: "ap-hint" }, desc),
        ...(items.length
          ? items
          : [h("p", { class: "ap-empty" }, "Nothing here.")]),
      );
    const deployItems = p.deploy.map((s) => {
      const box = drivable(
        h("input", {
          type: "checkbox",
          "aria-label": `Include ${s}`,
          title: "Include this stack in the batch",
        }),
        PICK,
        s,
      );
      /** @type {HTMLInputElement} */ (box).checked = ticked.has(s);
      box.addEventListener("change", () => {
        if (/** @type {HTMLInputElement} */ (box).checked) ticked.add(s);
        else ticked.delete(s);
        paint();
      });
      const more = drivable(
        h(
          "button",
          {
            type: "button",
            class: "ap-link",
            "aria-expanded": String(open === s),
            title: "The files deploying this stack sends, as a diff",
          },
          open === s ? "Hide what changes" : "Show what changes",
        ),
        DIFF,
        s,
      );
      more.addEventListener("click", () => {
        open = open === s ? null : s;
        if (open === s && !diffs.has(s)) void loadDiff(s);
        paint();
      });
      return h(
        "div",
        { class: "ap-item", "data-stack": s },
        h(
          "div",
          { class: "ap-item__top" },
          box,
          stackMark(s, 16),
          h("a", { href: stackHref(s) }, h("strong", null, s)),
          p.new.includes(s)
            ? h("span", { class: "kp-badge kp-badge--info" }, "new")
            : h("span"),
        ),
        h(
          "p",
          null,
          p.reasons?.[s] ??
            (p.new.includes(s)
              ? "new: creates its container"
              : "its files differ from what the host applied"),
        ),
        more,
        ...(open === s
          ? [h("div", { class: "ap-diff" }, ...(diffs.get(s) ?? []))]
          : []),
      );
    });
    const destroyItems = p.destroy.map((s) => {
      const armed = typedText.get(s) === s;
      const input = h("input", {
        class: "kp-field__input",
        id: fieldId(TYPE, s),
        placeholder: `type ${s} to arm it`,
        "aria-label": `Type ${s} to destroy it`,
        autocomplete: "off",
        spellcheck: "false",
      });
      /** @type {HTMLInputElement} */ (input).value = typedText.get(s) ?? "";
      input.addEventListener("input", () => {
        const v = /** @type {HTMLInputElement} */ (input).value;
        const was = typedText.get(s) === s;
        typedText.set(s, v);
        if ((v === s) !== was) {
          paint();
          /** @type {HTMLInputElement | null} */ (
            body.querySelector(
              `[aria-label="Type ${CSS.escape(s)} to destroy it"]`,
            )
          )?.focus();
        }
      });
      const id = vmid(s);
      return h(
        "div",
        {
          class: `ap-item ap-item--destroy${armed ? " armed" : ""}`,
          "data-stack": s,
        },
        h(
          "div",
          { class: "ap-item__top" },
          // An unarmed destroy is neutral, not information (review item 12).
          h("span", {
            class: `nx-sev ${armed ? "nx-sev--bad" : "ap-sev--none"}`,
            "aria-hidden": "true",
          }),
          stackMark(s, 16),
          h("strong", null, s),
          h(
            "span",
            { class: `kp-badge${armed ? " kp-badge--destructive" : ""}` },
            armed ? "armed" : "left alone",
          ),
        ),
        h(
          "p",
          null,
          `Stops and removes ${id != null ? `CT ${id}` : "its container"}; its data is kept under Backups ▸ Kept from removed stacks until you wipe it.`,
        ),
        input,
      );
    });
    const brokenItems = p.broken.map(([s, why]) => {
      const [what, fix] = String(why).split(" :: ");
      return h(
        "div",
        { class: "ap-item ap-item--broken", "data-stack": s },
        h(
          "div",
          { class: "ap-item__top" },
          h("span", { class: "nx-sev", "aria-hidden": "true" }),
          stackMark(s, 16),
          h("strong", null, s),
          h("span"),
        ),
        h("p", null, what),
        // The working copy's own path says nothing to a person: from
        // `stacks/` on, as the repository names it.
        ...(fix
          ? [
              h(
                "p",
                null,
                h("strong", null, "Fix: "),
                fix.replace(/\S*\/repo\/(stacks\/)/g, "$1"),
              ),
            ]
          : []),
        drivable(
          h(
            "a",
            {
              class: "kp-button kp-button--sm",
              href: `${stackHref(s, "settings")}?section=files`,
              title: "Open this stack's file on its Settings tab",
            },
            "Open the stack file",
          ),
          FILE,
          s,
        ),
      );
    });
    const showAll = only
      ? [
          drivable(
            h(
              "button",
              {
                type: "button",
                class: "ap-link ap-showall",
                title: "Show every column again (Esc)",
              },
              "Show all",
            ),
            SHOW_ALL,
          ),
        ]
      : [];
    showAll[0]?.addEventListener("click", () => {
      only = null;
      paint();
    });
    body.replaceChildren(
      h(
        "section",
        { class: "ap-counts", "aria-label": "Plan summary" },
        tile(
          "deploy",
          "Will deploy",
          p.deploy.length,
          `${p.new.length} new stack${p.new.length === 1 ? "" : "s"}`,
        ),
        tile(
          "destroy",
          "Will be destroyed",
          p.destroy.length,
          "directory gone from the files",
          "bad",
        ),
        tile(
          "broken",
          "Cannot be planned",
          p.broken.length,
          "fix the file, then plan again",
          "warn",
        ),
        tile("same", "Unchanged", p.unchanged.length, "nothing to do"),
      ),
      ...(showAll.length
        ? [h("p", { class: "ap-filter" }, `Showing one column. `, ...showAll)]
        : []),
      h(
        "div",
        { class: "ap-cols" },
        col(
          "deploy",
          "Will deploy",
          "info",
          p.deploy.length,
          "Created or brought in line with its files. Untick one to leave it out of this batch.",
          deployItems,
        ),
        col(
          "destroy",
          "Will be destroyed",
          "bad",
          p.destroy.length,
          "Their directory is gone from the working copy. Type a stack's name to arm it for the separate Destroy step below.",
          destroyItems,
        ),
        col(
          "broken",
          "Cannot be planned",
          "warn",
          p.broken.length,
          "These stack files did not read. They are left alone; fix them and plan again.",
          brokenItems,
        ),
        ...(only === "same"
          ? [
              h(
                "section",
                {
                  class: "ap-col",
                  "aria-label": "Unchanged",
                  "data-col": "same",
                },
                h(
                  "h3",
                  null,
                  "Unchanged",
                  h("span", { class: "kp-badge" }, String(p.unchanged.length)),
                ),
                h(
                  "p",
                  { class: "ap-hint" },
                  p.unchanged.length ? p.unchanged.join(", ") : "Nothing here.",
                ),
              ),
            ]
          : []),
      ),
    );
    const g = goPlan(p, ticked);
    const apply = viaForm(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--primary",
          id: "apply-open",
          ...(g.n === 0 ? { disabled: "" } : {}),
          title:
            "Deploy the ticked stacks after one confirmation; each stack is backed up first. Nothing is destroyed here.",
        },
        g.label,
      ),
      "apply",
    );
    apply.addEventListener("click", () => {
      pressedAt = Date.now() / 1000 - 2;
      applied = "pressed";
      void openAction("_host", "apply", { preset: g.preset });
    });
    go.replaceChildren(
      h("div", null, h("strong", null, g.title), h("p", null, g.note)),
      h("div", { class: "ap-go__acts" }, apply),
    );
    paintDestroy(p, g.n > 0 && applied !== "done");
  };

  /** @param {Plan} p @param {boolean} deploysPending */
  const paintDestroy = (p, deploysPending) => {
    if (!p.destroy.length) {
      destroyBox.hidden = true;
      destroyBox.replaceChildren();
      return;
    }
    destroyBox.hidden = false;
    const d = destroyStep(p, typed(), vmid, { ack, deploysPending });
    if (!d.armed.length) ack = false;
    const tick = drivable(
      h("input", {
        type: "checkbox",
        id: DESTROY_ACK_FIELD,
        ...(d.armed.length ? {} : { disabled: "" }),
      }),
      DESTROY_ACK,
    );
    /** @type {HTMLInputElement} */ (tick).checked = ack && d.armed.length > 0;
    tick.addEventListener("change", () => {
      ack = /** @type {HTMLInputElement} */ (tick).checked;
      paintDestroy(p, deploysPending);
    });
    const button = drivable(
      h(
        "button",
        {
          type: "button",
          class: "kp-button kp-button--destructive",
          id: "apply-destroy",
          ...(d.ready ? {} : { disabled: "" }),
          title:
            "Destroy the armed stacks after one more confirmation: each is backed up and its backup restored as a check first",
        },
        d.label,
      ),
      DESTROY,
    );
    button.addEventListener("click", () => {
      void openAction("_host", "apply", { preset: d.preset });
    });
    destroyBox.replaceChildren(
      h(
        "div",
        { class: "ap-destroy__head" },
        h("span", { class: "nx-sev nx-sev--bad", "aria-hidden": "true" }),
        h("h3", null, "Destroy"),
        h(
          "p",
          { class: "ap-hint" },
          "A separate step after the deploys: only the stacks you armed by typing their name, only after this confirmation, each backed up and restore-checked first.",
        ),
      ),
      h("strong", null, d.title),
      h(
        "label",
        { class: "ap-destroy__ack", for: "apply-destroy-ack" },
        tick,
        h("span", null, d.ackLabel),
      ),
      h(
        "div",
        { class: "ap-destroy__go" },
        h(
          "p",
          { class: "ap-hint", role: "status" },
          d.why ?? "Ready to destroy.",
        ),
        button,
      ),
    );
  };

  // The deploys this page started: once that job is done the plan is read
  // again, so the destroy step knows no ticked deploy is waiting.
  const stopJobs = onAct("jobs", () => {
    if (pressedAt == null) return;
    const j = act.jobs.find(
      (x) =>
        x.action === "apply" &&
        x.queued_at >= /** @type {number} */ (pressedAt) &&
        !String(x.args?.destroy ?? ""),
    );
    if (!j) return;
    if (j.state === "queued" || j.state === "running") {
      if (applied !== "running") {
        applied = "running";
        paint();
      }
      return;
    }
    pressedAt = null;
    applied = j.state === "done" ? "done" : "none";
    void load().catch(() => {});
  });

  // Esc resets the tile filter (bound on mount, released on unmount).
  /** @param {KeyboardEvent} e */
  const onKey = (e) => {
    if (e.key !== "Escape" || only == null || e.defaultPrevented) return;
    // redesign-final (a case red on the base): the plan lives in the Deploy
    // all changes sheet since 3.71.0; Esc there resets the filter first
    // and closes the sheet only once no filter is on. Another dialog on
    // top keeps its own Esc.
    const top = [...document.querySelectorAll("dialog[open]")].pop();
    if (top && !top.contains(root)) return;
    e.preventDefault();
    only = null;
    paint();
  };
  document.addEventListener("keydown", onKey, true);

  const load = async () => {
    read.setAttribute("disabled", "");
    body.replaceChildren(skeleton());
    go.replaceChildren(
      h(
        "p",
        { class: "ap-hint" },
        "Reading every stack (latch is asked for the secrets the host's hash covers)…",
      ),
    );
    const r = await fetchJson("/data/apply/plan", "the plan", abort.signal);
    if (abort.signal.aborted) return;
    read.removeAttribute("disabled");
    if (!r.ok) {
      err.replaceChildren(errorBox(r.error));
      body.replaceChildren();
      go.replaceChildren();
      return;
    }
    err.replaceChildren();
    plan = /** @type {Plan} */ (r.body.plan);
    ticked = new Set(plan.deploy);
    diffs.clear();
    paint();
    opts.onRead?.({
      pending: plan.deploy.length + plan.destroy.length,
      broken: plan.broken.length,
    });
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  read.addEventListener("click", () => void load().catch(() => {}));
  paintSteps();
  void load().catch(() => {});
  return () => {
    abort.abort();
    stopCompare();
    stopJobs();
    document.removeEventListener("keydown", onKey, true);
  };
}
