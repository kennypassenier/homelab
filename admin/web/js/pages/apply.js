// Deploy all changes (TUI parity, ask-8's `homelab apply`, dash-apply:
// "Yes, plan first"; renamed from "Apply the whole fleet" in 3.71.0): every
// stack of the working copy against what the host last applied.
//
// Rebuilt on the approved kp-themes 11 demo (feat-kp11demo-13): the plan is
// one kp datatable, a row per stack with its action (deploy, new, destroy,
// cannot be planned, unchanged), what changed, the files and +N −M, and a
// row that opens to the exact lines (kp diff) of each file; one kp selection
// bar and a sentence under the table ("Will deploy 5 stacks (2 new), destroy
// 0, back up first") say what Apply will do before it is pressed. A destroy
// never rides along with a deploy: Apply turns off and says so, and "Apply
// the destroy alone" asks for the stack's name to be typed. The plan's own
// steps (Compare, Review, Apply, Destroy) and "Plan again" keep their places
// above the table. Pure half: js/applyplan.js, js/applytable.js.
//
// `mount` paints only the section's body (the caller, Stacks' "Deploy all
// changes", owns its heading and one-line description) and starts loading
// the table's own way before the plan has been read.

import { act, onAct } from "../act.js";
import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { destroyStep, goPlan } from "../applyplan.js";
import {
  applyRule,
  diffFiles,
  planRows,
  plusMinus,
  sentenceCounts,
  sentenceParts,
} from "../applytable.js";
import {
  ensureStyle,
  errorBox,
  fetchJson,
  h,
  labelCells,
  selectCell,
  tableBlock,
} from "../dom.js";
import { declare, declareField, drivable, viaForm } from "../drivable.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";
import { current } from "../store.js";
import { bindSelection } from "../selectbind.js";
import { openConfirmation } from "/static/kp/js/components.js";
import { attachDataTables } from "/static/kp/js/datatable.js";
import { attachIdmarks } from "/static/kp/js/idmark.js";
import { attachSentences } from "/static/kp/js/sentence.js";

const SKIP_FIELD = declareField({
  id: "apply-skip-backups",
  page: "apply",
  what: "whether to back up the stacks first or skip the backups",
});
const READ = declare({
  id: "deploy-all-plan-again",
  page: "overview",
  opens: "run",
  what: "read the plan of every stack again from the host and the working copy",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const PICK = declare({
  id: "deploy-all-pick",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "include or leave out one stack of the batch (a destroy is armed by ticking it)",
  selects: true,
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const PICK_ALL = declare({
  id: "deploy-all-pick-all",
  page: "overview",
  opens: "view",
  what: "tick every stack that can be applied",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const DIFF = declare({
  id: "deploy-all-diff",
  page: "overview",
  opens: "view",
  row: "<stack>",
  what: "open or close the exact lines applying one stack changes",
  shows: "in the Deploy all changes panel",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
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
const UNTICK = declare({
  id: "deploy-all-untick",
  page: "overview",
  opens: "view",
  what: "untick every ticked stack of the plan",
  shows: "in the Deploy all changes panel, while a stack is ticked",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const SKIP = declare({
  id: "deploy-all-skip-backups",
  page: "overview",
  opens: "view",
  row: "back-up-first|skip",
  what: "choose, in the sentence under the table, whether the deploys back each stack up first or skip the backups (back up first is the default)",
  shows: "in the Deploy all changes panel, once the plan is read",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const APPLY = declare({
  id: "deploy-all-apply",
  page: "overview",
  opens: "dialog",
  what: "apply the ticked deploys: one confirmed batch, each stack backed up first unless skipped",
  shows: "in the Deploy all changes panel, while stacks are ticked",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});
const DESTROY = declare({
  id: "deploy-all-destroy",
  page: "overview",
  opens: "dialog",
  what: "apply the ticked destroy on its own: the stack's name is typed, then the apply dialog confirms once more",
  shows: "in the Deploy all changes panel, while only a destroy is ticked",
  reach: [{ do: "click", control: "stacks-deploy-all" }],
});

/**
 * @typedef {import("../applyplan.js").Plan} Plan
 * @typedef {{button: HTMLElement, at: () => number | null,
 *   onChange: (f: () => void) => () => void}} Compare
 */

export { goPlan } from "../applyplan.js";

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

  // The plan: one kp datatable with a row per stack, opened for its lines.
  const block = tableBlock({
    remember: "deploy-all-plan",
    caption: "What Apply would change; open a row to read the lines",
    captionHidden: true,
    search: null,
    nothing: "Nothing to apply: the repository and the host agree.",
    select: { label: "Select every stack that can be applied", actions: [] },
    columns: [
      { label: "Stack", sort: "text" },
      { label: "Action", sort: "text" },
      { label: "What changed", sort: null },
      { label: "Files", sort: "number", cls: "kp-text-end" },
      { label: "Lines", sort: "number", cls: "kp-text-end" },
    ],
  });
  block.wrap.querySelector("[data-kp-datatable-actions]")?.remove();
  block.wrap.setAttribute("data-kp-expandable", "");
  block.wrap.setAttribute("data-kp-expand-single", "");
  block.wrap.id = "ap-plan";
  const pickAll = block.wrap.querySelector("[data-kp-select-all]");
  if (pickAll instanceof HTMLElement) drivable(pickAll, PICK_ALL);
  const bar = h(
    "div",
    {
      class: "kp-selection ap-bar",
      "data-kp-selection": "",
      "data-kp-selection-for": "ap-plan",
      "data-kp-placement": "sticky-top",
      id: "ap-bar",
      "aria-label": "Stacks to apply",
    },
    h(
      "div",
      { class: "kp-selection__actions" },
      drivable(
        viaForm(
          h(
            "button",
            {
              type: "button",
              class: "kp-button kp-button--primary",
              id: "apply-open",
              "data-kp-selection-action": "apply",
              title: "Deploy the ticked stacks after one confirmation",
            },
            "Apply",
          ),
          "apply",
        ),
        APPLY,
      ),
      drivable(
        h(
          "button",
          {
            type: "button",
            class: "kp-button kp-button--destructive",
            id: "apply-destroy",
            "data-kp-selection-action": "destroy",
            hidden: "",
            title:
              "Destroy the ticked stack on its own, after its name is typed",
          },
          "Apply the destroy alone",
        ),
        DESTROY,
      ),
    ),
  );
  const words = h("p", { class: "kp-sentence__text", id: "ap-plan-words" });
  const form = h(
    "form",
    {
      class: "kp-sentence ap-sentence",
      "data-kp-sentence": "",
      id: "ap-plan-sentence",
    },
    words,
  );
  form.addEventListener("submit", (e) => e.preventDefault());
  root.replaceChildren(
    steps,
    h(
      "div",
      { class: "ap-top" },
      h("div", { class: "ap-top__right" }, ago, read),
    ),
    err,
    bar,
    block.wrap,
    form,
  );
  const abort = new AbortController();
  const detachTables = attachDataTables(root, {
    decorate: (part, info) => {
      if (info.kind === "row-toggle") drivable(part, DIFF, String(info.value));
    },
  });
  const detachMarks = attachIdmarks(root);

  /** @type {Plan | null} */
  let plan = null;
  /** @type {Map<string, import("../plan.js").FileDiff[] | null>} the diffs read so far (null: could not be read) */
  const diffs = new Map();
  // Back up first is the default; Skip the backups is an option, never
  // remembered (Kenny, 2026-10-04).
  let skipBackups = false;
  /** @type {"none" | "pressed" | "running" | "done"} */
  let applied = "none";
  /** @type {number | null} unix seconds of this page's Apply press */
  let pressedAt = null;
  /** @type {string[]} */
  let keys = [];

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
      if (plan.destroy.length && keys.some((k) => plan?.destroy.includes(k)))
        mark("destroy", "armed");
    } else mark("compare", "on");
    for (const l of list)
      if (l.classList.contains("on")) l.setAttribute("aria-current", "step");
    steps.style.setProperty("--ap-steps", String(list.length));
    steps.replaceChildren(...list);
  };
  const stopCompare = opts.compare?.onChange(paintSteps) ?? (() => {});

  /** The sentence: counts, then the choice of backing up as a menu inside it. */
  const sentence = () => {
    const sel = form.querySelector("select");
    const keep = sel?.value ?? (skipBackups ? "skip" : "yes");
    const c = plan
      ? sentenceCounts(plan, keys)
      : { deploy: 0, fresh: 0, destroy: 0 };
    const part = sentenceParts(c).map((x) =>
      "bold" in x ? h("b", null, x.bold) : x.text,
    );
    const select = h(
      "select",
      {
        name: "backup",
        "aria-label": "Backups before applying",
        id: SKIP_FIELD,
      },
      h(
        "option",
        { value: "yes", ...(keep === "yes" ? { selected: "" } : {}) },
        "back up first",
      ),
      h(
        "option",
        { value: "skip", ...(keep === "skip" ? { selected: "" } : {}) },
        "skip backups",
      ),
    );
    drivable(select, SKIP, keep === "skip" ? "skip" : "back-up-first");
    words.replaceChildren(
      ...part,
      h("span", { class: "kp-sentence__field" }, select),
      ".",
    );
    attachSentences(form.parentElement ?? document);
    select.addEventListener("change", () => {
      skipBackups = select.value === "skip";
    });
  };

  /** @param {string} s */
  const loadDiff = async (s) => {
    if (diffs.has(s)) return;
    const r = await fetchJson(
      `/data/plan/${encodeURIComponent(s)}`,
      `the plan of ${s}`,
      abort.signal,
    );
    if (abort.signal.aborted) return;
    diffs.set(s, r.ok ? (r.body.files ?? []) : null);
    if (plan) draw();
  };

  /** Draw the table from the plan and the diffs read so far. */
  const draw = () => {
    paintSteps();
    const p = plan;
    if (!p) return;
    const rows = planRows(p, vmid);
    block.tbody.replaceChildren(
      ...rows.flatMap((r) => {
        const off = r.action === "unchanged" || r.action === "broken";
        const tick = selectCell(r.stack, `Select ${r.stack}`);
        const box = /** @type {HTMLInputElement} */ (
          tick.querySelector("input")
        );
        drivable(box, PICK, r.stack);
        if (off) {
          box.disabled = true;
          box.title =
            r.action === "broken"
              ? "Cannot be planned: its file did not read"
              : "Nothing changed: there is nothing to apply";
        }
        const files = diffs.get(r.stack);
        const pm = files ? plusMinus(files) : null;
        const tone =
          r.action === "destroy"
            ? " kp-badge--destructive"
            : r.action === "new"
              ? " kp-badge--success"
              : r.action === "broken"
                ? " kp-badge--warning"
                : r.action === "unchanged"
                  ? ""
                  : " kp-badge--info";
        return [
          h(
            "tr",
            { "data-kp-row-key": r.stack, "data-kp-row-label": r.stack },
            tick,
            h(
              "td",
              { "data-kp-sort-value": r.stack },
              h(
                "span",
                { class: "ap-name" },
                h("span", {
                  class: "kp-idmark kp-idmark--sm",
                  "data-kp-idmark": r.stack,
                  "aria-hidden": "true",
                }),
                h("a", { href: stackHref(r.stack) }, h("b", null, r.stack)),
              ),
            ),
            h(
              "td",
              { "data-kp-sort-value": r.action },
              h(
                "span",
                { class: `kp-badge${tone}` },
                r.action === "broken" ? "cannot be planned" : r.action,
              ),
            ),
            h("td", null, r.note),
            h(
              "td",
              {
                class: "kp-text-end",
                "data-kp-sort-value": String(files?.length ?? 0),
              },
              files ? String(files.length) : files === null ? "—" : "…",
            ),
            h(
              "td",
              {
                class: "kp-text-end ap-nowrap",
                "data-kp-sort-value": String(pm ? pm.add + pm.del : 0),
              },
              pm
                ? [
                    h("span", { class: "ap-add" }, `+${pm.add}`),
                    " ",
                    h("span", { class: "ap-del" }, `\u2212${pm.del}`),
                  ]
                : files === null
                  ? "—"
                  : "…",
            ),
          ),
          h(
            "tr",
            { "data-kp-row-detail": "" },
            h(
              "td",
              { colspan: "6" },
              r.action === "broken"
                ? h(
                    "div",
                    { class: "ap-broken" },
                    r.fix ? h("p", null, h("b", null, "Fix: "), r.fix) : null,
                    drivable(
                      h(
                        "a",
                        {
                          class: "kp-button kp-button--sm",
                          href: `${stackHref(r.stack, "settings")}?section=files`,
                          title: "Open this stack's file on its Settings tab",
                        },
                        "Open the stack file",
                      ),
                      FILE,
                      r.stack,
                    ),
                  )
                : r.action === "unchanged"
                  ? h(
                      "p",
                      { class: "ap-none" },
                      "No file changed since the last apply.",
                    )
                  : files === undefined
                    ? h("p", { class: "ap-none" }, "Reading the lines…")
                    : files === null
                      ? h(
                          "p",
                          { class: "ap-none" },
                          "The lines could not be read for this stack.",
                        )
                      : diffFiles(files),
            ),
          ),
        ];
      }),
    );
    labelCells(block.tbody, () => [
      "",
      "Stack",
      "Action",
      "What changed",
      "Files",
      "Lines",
    ]);
    block.ready();
    // The deploys start ticked; a destroy only when someone arms it.
    for (const b of /** @type {NodeListOf<HTMLInputElement>} */ (
      block.tbody.querySelectorAll("input[data-kp-select-row]")
    )) {
      const want = ticked0.has(b.value) && !b.disabled;
      if (b.checked !== want) {
        b.checked = want;
        b.dispatchEvent(new Event("change", { bubbles: true }));
      }
    }
    sentence();
  };
  /** @type {Set<string>} what is ticked as the table is drawn (kept across a redraw) */
  let ticked0 = new Set();

  bar.addEventListener("kp-selection-change", (e) => {
    keys = /** @type {CustomEvent} */ (e).detail.keys;
    ticked0 = new Set(keys);
    const onlyDestroy =
      plan != null &&
      keys.length > 0 &&
      keys.every((k) => plan?.destroy.includes(k));
    const apply = bar.querySelector('[data-kp-selection-action="apply"]');
    const destroy = bar.querySelector('[data-kp-selection-action="destroy"]');
    if (apply instanceof HTMLElement) apply.hidden = onlyDestroy;
    if (destroy instanceof HTMLElement) destroy.hidden = !onlyDestroy;
    sentence();
    // Opening a row reads that stack's lines.
  });
  const sel = bindSelection(
    root,
    bar,
    () => ({
      apply: (items) =>
        plan
          ? applyRule(
              plan,
              items.map((i) => i.key),
            )
          : "",
    }),
    undefined,
    (c) => drivable(c, UNTICK),
  );
  block.wrap.addEventListener("kp-datatable-expand", (e) => {
    const d = /** @type {CustomEvent} */ (e).detail;
    if (d.open && d.key) void loadDiff(String(d.key)).catch(() => {});
  });
  bar.addEventListener("kp-selection-action", (e) => {
    const { action, button, keys: ks } = /** @type {CustomEvent} */ (e).detail;
    const p = plan;
    if (!p) return;
    if (action === "apply") {
      const g = goPlan(p, new Set(ks), skipBackups);
      pressedAt = Date.now() / 1000 - 2;
      applied = "pressed";
      void openAction("_host", "apply", { preset: g.preset });
      return;
    }
    const gone = ks.filter((/** @type {string} */ k) => p.destroy.includes(k));
    openConfirmation(button, {
      phrase: `Destroy ${gone.length === 1 ? `the stack ${gone[0]}` : `${gone.length} stacks`}? Each is backed up first.`,
      typeName: gone.length === 1 ? gone[0] : `destroy ${gone.length} stacks`,
      loses: gone.map((/** @type {string} */ s) => {
        const id = vmid(s);
        return `${s}: ${id != null ? `CT ${id}` : "its container"} and its place in the gateway`;
      }),
      acceptLabel:
        gone.length === 1
          ? `Destroy ${gone[0]}`
          : `Destroy ${gone.length} stacks`,
      onAccept: () => {
        const d = destroyStep(p, new Set(gone), vmid, {
          ack: true,
          deploysPending: false,
        });
        void openAction("_host", "apply", { preset: d.preset });
      },
    });
  });

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
        paintSteps();
      }
      return;
    }
    pressedAt = null;
    applied = j.state === "done" ? "done" : "none";
    void load().catch(() => {});
  });

  const load = async () => {
    read.setAttribute("disabled", "");
    block.loading({
      words:
        "Working out the plan of every stack (latch is asked for the secrets the host's hash covers)…",
      expect: 20,
    });
    const r = await fetchJson("/data/apply/plan", "the plan", abort.signal);
    if (abort.signal.aborted) return;
    read.removeAttribute("disabled");
    if (!r.ok) {
      err.replaceChildren();
      block.failed(r.error);
      return;
    }
    err.replaceChildren();
    plan = /** @type {Plan} */ (r.body.plan);
    ticked0 = new Set(plan.deploy);
    diffs.clear();
    draw();
    // The lines of every stack that would change are read at once, so the
    // Files and Lines columns are filled without opening a row.
    for (const s of [...plan.deploy, ...plan.destroy])
      void loadDiff(s).catch(() => {});
    opts.onRead?.({
      pending: plan.deploy.length + plan.destroy.length,
      broken: plan.broken.length,
    });
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  block.wrap.addEventListener(
    "kp-datatable-retry",
    () => void load().catch(() => {}),
  );
  read.addEventListener("click", () => void load().catch(() => {}));
  paintSteps();
  sentence();
  void load().catch(() => {});
  return () => {
    abort.abort();
    stopCompare();
    stopJobs();
    detachTables();
    sel.stop();
    detachMarks();
  };
}
