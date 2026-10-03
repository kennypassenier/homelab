// Deploy all changes (TUI parity, ask-8's `homelab apply`, dash-apply:
// "Yes, plan first"): every stack of the working copy against what the
// host last applied, each changed stack's diff on request, then one
// confirmation, the apply form, where a stack whose directory is gone is
// destroyed only after its name is typed.
//
// fix-210 made this a folded section of Overview; redesign-stacks (3.71.0,
// FLOWS.md §1 row 3 and task 13, Kenny approved 2026-10-03) makes it the
// body of the Stacks page's "Deploy all changes" side panel: the Stacks
// header button says the count before you click, the panel reads the plan
// only once it is opened (a skeleton first), and its foot holds the one
// button that opens the existing apply dialog. `/apply` and
// `/overview?section=apply` still land here (router.js `redirectFor`,
// `/stacks?deploy-all=1`); Live view's `homelab ui open apply` still works
// because "apply" stays a known form in `formspec.json`.

import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { errorBox, fetchJson, h } from "../dom.js";
import { diffBlocks } from "../editui.js";
import { applySummary } from "../parity.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";
import { dialogControl, viaForm } from "../drivable.js";

/** A handful of skeleton rows shown the moment the panel opens, so there
 * is never a blank body while the first plan read is in flight. */
function skeletonGrid() {
  return h(
    "div",
    {
      class: "apply-grid apply-grid--skeleton",
      role: "status",
      "aria-label": "Reading the plan",
      "data-kp-state": "loading",
    },
    ...Array.from({ length: 3 }, () =>
      h("div", { class: "apply-row apply-row--skeleton kp-skeleton" }),
    ),
  );
}

/**
 * The plan's body. `foot` (the panel's foot) receives the two buttons —
 * "Read the plan again" and the one that opens the apply dialog; without
 * it they sit at the top of the body.
 * @param {HTMLElement} root
 * @param {{foot?: HTMLElement, onRead?: (pending: number) => void}} [opts]
 * @returns {() => void}
 */
export function mount(root, opts = {}) {
  const read = h(
    "button",
    {
      type: "button",
      class: "kp-button",
      id: "apply-read",
      title: "Read the plan again from the host and the working copy.",
    },
    "Read the plan again",
  );
  const go = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--primary",
      id: "apply-open",
      disabled: "",
      title:
        "Deploy every pending change at once, after one confirmation; each stack is backed up first, and a stack whose directory vanished is destroyed only once its name is typed there.",
    },
    "Deploy all changes…",
  );
  const head = h("div", { "aria-live": "polite", id: "apply-head" });
  const err = h("div");
  const body = h("div", { class: "apply-grid", id: "apply-body" });
  const ago = agoEl("read");
  const nothingYet = h(
    "span",
    { class: "sk-hint" },
    "Nothing runs until the dialog is confirmed.",
  );
  if (opts.foot) opts.foot.replaceChildren(nothingYet, read, go);
  root.replaceChildren(
    ...(opts.foot
      ? []
      : [
          h(
            "div",
            { class: "title-row" },
            h("span", { class: "actions-row" }, read, go),
          ),
        ]),
    err,
    head,
    body,
    h("p", { class: "sk-hint" }, ago),
  );
  body.replaceChildren(skeletonGrid());
  const abort = new AbortController();

  /**
   * @param {string} stack
   * @param {boolean} isNew
   * @param {string | undefined} reason fix-192: which component differs
   *   (files, env, secrets, or only the derived manifest), read straight
   *   from the plan — never recomputed in the page.
   */
  const stackBlock = (stack, isNew = false, reason = undefined) => {
    const diffBody = h(
      "div",
      { class: "apply-diff" },
      h("p", { class: "measured" }, "Open to read the diff."),
    );
    const d = h(
      "details",
      { class: "plan-file apply-stack apply-row", "data-stack": stack },
      h(
        "summary",
        null,
        h("a", { href: stackHref(stack) }, stack),
        isNew
          ? " · new container"
          : ` · ${reason ?? "files differ from what the host applied"}`,
      ),
      diffBody,
    );
    let loaded = false;
    d.addEventListener("toggle", async () => {
      if (!d.open || loaded) return;
      loaded = true;
      diffBody.replaceChildren(h("p", { class: "measured" }, "Reading…"));
      const r = await fetchJson(
        `/data/plan/${encodeURIComponent(stack)}`,
        `the plan of ${stack}`,
        abort.signal,
      );
      if (!r.ok) {
        diffBody.replaceChildren(errorBox(r.error));
        loaded = false;
        return;
      }
      const files = fileViews(r.body.files ?? []);
      diffBody.replaceChildren(
        h("p", { class: "measured" }, r.body.note),
        ...(files.length
          ? diffBlocks(files)
          : [h("p", null, "No file changes.")]),
      );
    });
    return d;
  };

  const load = async () => {
    read.disabled = true;
    go.disabled = true;
    body.replaceChildren(skeletonGrid());
    head.replaceChildren(
      h(
        "p",
        { class: "measured" },
        "Reading every stack against its files (latch is asked for the secrets the host's hash covers)…",
      ),
    );
    const r = await fetchJson("/data/apply/plan", "the plan", abort.signal);
    read.disabled = false;
    if (!r.ok) {
      err.replaceChildren(errorBox(r.error));
      head.replaceChildren();
      body.replaceChildren();
      return;
    }
    err.replaceChildren();
    const p = r.body.plan;
    const s = applySummary(p);
    head.replaceChildren(
      h(
        "div",
        {
          class: `kp-alert kp-alert--${s.blocked ? "destructive" : s.pending ? "warning" : "success"}`,
          role: "status",
          "data-kp-semantic": "",
        },
        h(
          "strong",
          null,
          s.pending ? s.headline : "The host runs exactly what the files say.",
        ),
        ...(s.blocked ? [h("span", { class: "refusal-line" }, s.blocked)] : []),
      ),
    );
    /** @type {HTMLElement[]} */
    const groups = [];
    if (p.deploy.length)
      groups.push(
        h(
          "div",
          { class: "apply-group" },
          h("h3", null, "To deploy"),
          ...p.deploy.map((/** @type {string} */ n) =>
            stackBlock(n, p.new.includes(n), p.reasons?.[n]),
          ),
        ),
      );
    if (p.destroy.length)
      groups.push(
        h(
          "div",
          { class: "apply-group" },
          h("h3", null, "Gone from the files"),
          ...p.destroy.map((/** @type {string} */ n) =>
            h(
              "p",
              { class: "apply-row" },
              h("strong", null, n),
              " runs on the host; its directory is gone. Destroyed only when its name is typed in the deploy dialog (backed up first, its data kept).",
            ),
          ),
        ),
      );
    groups.push(
      h(
        "div",
        { class: "apply-group" },
        h("h3", null, "The rest"),
        ...s.lines
          .filter((l) => !l.startsWith("↑") && !l.startsWith("✗"))
          .map((l) => h("p", { class: "apply-row measured" }, l)),
      ),
    );
    body.replaceChildren(...groups);
    go.disabled = !s.pending || !!s.blocked;
    nothingYet.textContent = s.pending
      ? "Nothing has run yet: the dialog asks once more before it deploys."
      : "Nothing to deploy: every stack matches its files.";
    opts.onRead?.(p.deploy.length + p.destroy.length);
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  dialogControl(read, "read-plan");
  dialogControl(go, "deploy-all-changes");
  read.addEventListener("click", () => void load().catch(() => {}));
  // fix-239: Live view reaches it as `homelab ui open apply`.
  viaForm(go, "apply");
  go.addEventListener("click", () => void openAction("_host", "apply"));
  void load().catch(() => {});
  return () => abort.abort();
}
