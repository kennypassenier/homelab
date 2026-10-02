// Apply (TUI parity, ask-8's `homelab apply`, dash-apply: "Yes, plan
// first"): the whole stacks directory against the host. The plan per stack
// first, each changed stack's diff on request; then one confirmation, the
// apply form, where a stack whose directory is gone is destroyed only after
// its name is typed.

import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { errorBox, fetchJson, h } from "../dom.js";
import { diffBlocks } from "../editui.js";
import { applySummary } from "../parity.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  const read = h(
    "button",
    { type: "button", class: "kp-button", id: "apply-read" },
    "Read the plan again",
  );
  const go = h(
    "button",
    {
      type: "button",
      class: "kp-button kp-button--destructive",
      id: "apply-open",
      disabled: "",
    },
    "Apply…",
  );
  const head = h("div", { "aria-live": "polite", id: "apply-head" });
  const err = h("div");
  const deploy = h("div", { id: "apply-deploy" });
  const destroy = h("div", { id: "apply-destroy" });
  const rest = h("div", { id: "apply-rest" });
  const ago = agoEl("read");
  root.replaceChildren(
    h(
      "div",
      { class: "title-row" },
      h("h1", null, "Apply"),
      h("span", { class: "actions-row" }, read, go),
    ),
    h(
      "p",
      { class: "measured" },
      "Every stack of the working copy against what the host last applied, as homelab apply --plan shows it. Nothing runs until Apply… is confirmed; a stack whose directory is gone is destroyed only when its name is typed there.",
    ),
    err,
    head,
    deploy,
    destroy,
    rest,
    h("p", null, ago),
  );
  const abort = new AbortController();

  /**
   * @param {string} stack
   * @param {boolean} isNew
   * @param {string | undefined} reason fix-192: which component differs
   *   (files, env, secrets, or only the derived manifest), read straight
   *   from the plan — never recomputed in the page.
   */
  const stackBlock = (stack, isNew = false, reason = undefined) => {
    const body = h(
      "div",
      { class: "apply-diff" },
      h("p", { class: "measured" }, "Open to read the diff."),
    );
    const d = h(
      "details",
      { class: "plan-file apply-stack", "data-stack": stack },
      h(
        "summary",
        null,
        h("a", { href: stackHref(stack) }, stack),
        isNew
          ? " · new container"
          : ` · ${reason ?? "files differ from what the host applied"}`,
      ),
      body,
    );
    let loaded = false;
    d.addEventListener("toggle", async () => {
      if (!d.open || loaded) return;
      loaded = true;
      body.replaceChildren(h("p", { class: "measured" }, "Reading…"));
      const r = await fetchJson(
        `/data/plan/${encodeURIComponent(stack)}`,
        `the plan of ${stack}`,
        abort.signal,
      );
      if (!r.ok) {
        body.replaceChildren(errorBox(r.error));
        loaded = false;
        return;
      }
      const files = fileViews(r.body.files ?? []);
      body.replaceChildren(
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
    head.replaceChildren(
      h(
        "p",
        { class: "measured" },
        "Reading every stack (latch is asked for the secrets the host's hash covers)…",
      ),
    );
    const r = await fetchJson("/data/apply/plan", "the plan", abort.signal);
    read.disabled = false;
    if (!r.ok) {
      err.replaceChildren(errorBox(r.error));
      head.replaceChildren();
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
    deploy.replaceChildren(
      ...(p.deploy.length
        ? [
            h("h2", null, "To deploy"),
            ...p.deploy.map((/** @type {string} */ n) =>
              stackBlock(n, p.new.includes(n), p.reasons?.[n]),
            ),
          ]
        : []),
    );
    destroy.replaceChildren(
      ...(p.destroy.length
        ? [
            h("h2", null, "Gone from the files"),
            h(
              "ul",
              null,
              ...p.destroy.map((/** @type {string} */ n) =>
                h(
                  "li",
                  null,
                  h("strong", null, n),
                  " runs on the host; its directory is gone. Destroyed only when its name is typed in the apply form (backed up first, its data kept).",
                ),
              ),
            ),
          ]
        : []),
    );
    rest.replaceChildren(
      h("h2", null, "The rest"),
      h(
        "ul",
        null,
        ...s.lines
          .filter((l) => !l.startsWith("↑") && !l.startsWith("✗"))
          .map((l) => h("li", null, l)),
      ),
    );
    go.disabled = !s.pending || !!s.blocked;
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  read.addEventListener("click", () => void load().catch(() => {}));
  go.addEventListener("click", () => void openAction("_host", "apply"));
  void load().catch(() => {});
  return () => abort.abort();
}
