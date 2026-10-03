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
// Apply runs the host's own `apply` (one confirmed batch) when the whole
// plan is ticked; with stacks left out it deploys the ticked ones as a
// batch instead, because `apply` takes the whole plan or nothing — and so
// a destroy then waits for the whole plan. A plan with stacks that cannot
// be planned is refused by `apply`, so the same batch deploy leaves those
// alone.
//
// `mount` paints only the section's body (the caller — Stacks' "Deploy all
// changes" — owns its heading and one-line description) and starts with a
// skeleton in the final geometry before the plan has been read.

import { openAction, openBatch } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { errorBox, fetchJson, h } from "../dom.js";
import { declare, drivable, viaForm } from "../drivable.js";
import { diffBlocks } from "../editui.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";
import { current } from "../store.js";
import { stackMark } from "../ui.js";
import { ensureStyle } from "./flowskit.js";

const READ = declare({
  id: "deploy-all-plan-again",
  page: "overview",
  opens: "run",
  what: "read the plan of every stack again from the host and the working copy",
});
const TILE = declare({
  id: "deploy-all-tile",
  page: "overview",
  opens: "view",
  row: "deploy|destroy|broken|same",
  what: "show only one column of the plan (a plain click; again shows all)",
});
const PICK = declare({
  id: "deploy-all-pick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "include or leave out one stack of the deploy batch",
});
const DIFF = declare({
  id: "deploy-all-diff",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "show or hide what deploying one stack changes",
});
const TYPE = declare({
  id: "deploy-all-destroy-name",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "type a gone stack's name to include its destroy",
});
const FILE = declare({
  id: "deploy-all-stack-file",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open the stack file of a stack that cannot be planned (its Settings)",
});

/**
 * @typedef {{deploy: string[], new: string[], destroy: string[],
 *   broken: [string, string][], unchanged: string[], ephemeral: string[],
 *   reasons?: Record<string, string>}} Plan
 * @typedef {"deploy" | "destroy" | "broken" | "same"} Col
 */

/**
 * The go bar's words and what Apply does (pure).
 * @param {Plan} p
 * @param {Set<string>} ticked
 * @param {Set<string>} typed the gone stacks whose names were typed
 */
export function goPlan(p, ticked, typed) {
  const nDeploy = p.deploy.filter((s) => ticked.has(s)).length;
  const nDestroy = p.destroy.filter((s) => typed.has(s)).length;
  const whole =
    nDeploy === p.deploy.length &&
    p.broken.length === 0 &&
    nDeploy + nDestroy > 0;
  const mode = whole ? "apply" : nDeploy > 0 ? "batch" : "none";
  const left = [
    ...(p.broken.length
      ? [
          `${p.broken.length} ${p.broken.length === 1 ? "stack that cannot be planned is" : "stacks that cannot be planned are"} left alone`,
        ]
      : []),
    ...(nDeploy < p.deploy.length
      ? [`${p.deploy.length - nDeploy} left out of this batch`]
      : []),
  ];
  return {
    mode,
    nDeploy,
    nDestroy,
    title:
      mode === "none"
        ? nDestroy
          ? "A destroy runs only with the whole plan"
          : "Nothing selected to apply"
        : `Apply ${nDeploy} deploy${nDeploy === 1 ? "" : "s"} and ${mode === "apply" ? nDestroy : 0} of ${p.destroy.length} destroys`,
    note:
      mode === "apply"
        ? `${left.length ? `${left.join("; ")}. ` : ""}One confirmed batch; each stack is backed up first and shows its own progress in the stack list.`
        : mode === "batch"
          ? `${left.join("; ")}. The ticked stacks are deployed as one batch${nDestroy ? "; a destroy waits for the whole plan (tick every stack, fix what cannot be planned)" : ""}.`
          : nDestroy
            ? "Tick every stack to deploy and fix what cannot be planned; the host applies destroys only with the whole plan."
            : "Tick a stack to deploy, or type a gone stack's name to destroy it.",
    destructive: mode === "apply" && nDestroy > 0,
    label:
      mode === "apply" && nDestroy > 0
        ? `Apply and destroy ${nDestroy}…`
        : mode === "batch"
          ? `Deploy ${nDeploy}…`
          : "Apply…",
  };
}

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
 * @returns {() => void}
 */
export function mount(root) {
  ensureStyle("/css/pages/apply.css");
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
  const steps = h(
    "ol",
    { class: "ap-steps", "aria-label": "Steps" },
    h(
      "li",
      { class: "done" },
      h("strong", null, "Compare"),
      h("span", null, "Each stack against its files"),
    ),
    h(
      "li",
      { class: "on", "aria-current": "step" },
      h("strong", null, "Review the plan"),
      h("span", null, "What applying would change; nothing runs yet"),
    ),
    h(
      "li",
      null,
      h("strong", null, "Apply"),
      h("span", null, "One confirmed batch, each stack backed up first"),
    ),
  );
  const err = h("div");
  const body = h("div", { class: "ap-body", id: "apply-body" });
  const go = h("div", {
    class: "ap-go",
    role: "region",
    "aria-label": "Apply",
    id: "apply-head",
    "aria-live": "polite",
  });
  root.replaceChildren(
    h(
      "div",
      { class: "ap-top" },
      h(
        "p",
        { class: "section-head__desc" },
        "Every stack of the working copy against what the host last applied. Nothing runs until you press Apply; a stack whose directory is gone is destroyed only when you type its name.",
      ),
      h("div", { class: "ap-top__right" }, ago, read),
    ),
    steps,
    err,
    body,
    go,
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

  const typed = () =>
    new Set((plan?.destroy ?? []).filter((s) => typedText.get(s) === s));

  const vmid = (/** @type {string} */ s) =>
    current().fleet?.stacks.find((x) => x.name === s)?.vmid ?? null;

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
            title: "Click to show only this column; again to show all",
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
      const input = drivable(
        h("input", {
          class: "kp-field__input",
          placeholder: `type ${s} to include it`,
          "aria-label": `Type ${s} to destroy it`,
          autocomplete: "off",
          spellcheck: "false",
        }),
        TYPE,
        s,
      );
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
          h("span", {
            class: `nx-sev ${armed ? "nx-sev--bad" : "nx-sev--info"}`,
            "aria-hidden": "true",
          }),
          stackMark(s, 16),
          h("strong", null, s),
          h(
            "span",
            { class: `kp-badge${armed ? " kp-badge--destructive" : ""}` },
            armed ? "included" : "left out",
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
          "Their directory is gone from the working copy. Type a stack's name to include it; backups stay under Backups ▸ Kept from removed stacks.",
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
    const g = goPlan(p, ticked, typed());
    const apply = viaForm(
      h(
        "button",
        {
          type: "button",
          class: `kp-button ${g.destructive ? "kp-button--destructive" : "kp-button--primary"}`,
          id: "apply-open",
          ...(g.mode === "none" ? { disabled: "" } : {}),
          title:
            g.mode === "batch"
              ? "Deploy the ticked stacks as one batch, after one confirmation"
              : "Apply the whole plan after one confirmation; each stack is backed up first",
        },
        g.label,
      ),
      "apply",
    );
    apply.addEventListener("click", () => {
      if (g.mode === "apply")
        void openAction("_host", "apply", {
          preset: { destroy: [...typed()].join(",") },
        });
      else if (g.mode === "batch")
        void openBatch(
          "deploy",
          p.deploy.filter((s) => ticked.has(s)),
        );
    });
    go.replaceChildren(
      h("div", null, h("strong", null, g.title), h("p", null, g.note)),
      h("div", { class: "ap-go__acts" }, apply),
    );
  };

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
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  read.addEventListener("click", () => void load().catch(() => {}));
  void load().catch(() => {});
  return () => abort.abort();
}
