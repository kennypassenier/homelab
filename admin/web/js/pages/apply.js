// Apply the whole fleet (TUI parity, ask-8's `homelab apply`, dash-apply:
// "Yes, plan first"): every stack of the working copy against what the
// host last applied, each changed stack's diff on request, then one
// confirmation, the apply form, where a stack whose directory is gone is
// destroyed only after its name is typed.
//
// fix-210 (Kenny, 2026-10-02, "die functionaliteit kan toch ingebouwd
// worden in Overview?"): this used to be its own page at `/apply`; it is
// now a collapsible section of Overview (`overview.js`'s "Apply the whole
// fleet" section), mounted only once that section is opened, so visiting
// Overview never pays for a plan read nobody asked for. `/apply` itself
// still resolves (`router.js` `redirectFor`'s "apply" case sends it to
// `/overview?section=apply`, Live view's `homelab ui goto apply` still
// works because "apply" stays a known page in `formspec.json`).
//
// `mount` no longer paints its own `<h1>`/title row — the section's
// heading and one-line description are the caller's `<summary>`
// (`dom.js` `sectionHeader()`); this module owns only the section's body,
// starting with a skeleton grid shown the instant it is opened, before
// the plan has been read.

import { openAction } from "../actiondialog.js";
import { agoEl, setAgo } from "../ago.js";
import { errorBox, fetchJson, h } from "../dom.js";
import { diffBlocks } from "../editui.js";
import { applySummary } from "../parity.js";
import { fileViews } from "../plan.js";
import { stackHref } from "../router.js";

/** A handful of skeleton rows shown the moment the section opens, so there
 * is never a blank body while the first plan read is in flight — the same
 * shape (`kp-pulse`, reused from `backupcalendar.js`'s skeleton) as every
 * other page's first-paint loading state. */
function skeletonGrid() {
  return h(
    "div",
    { class: "apply-grid apply-grid--skeleton", "aria-hidden": "true" },
    ...Array.from({ length: 3 }, () =>
      h("div", { class: "apply-row apply-row--skeleton" }),
    ),
  );
}

/**
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
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
      class: "kp-button kp-button--destructive",
      id: "apply-open",
      disabled: "",
      title:
        "Apply every pending change at once, after one confirmation; a stack whose directory vanished is destroyed only once its name is typed there.",
    },
    "Apply…",
  );
  const head = h("div", { "aria-live": "polite", id: "apply-head" });
  const err = h("div");
  const body = h("div", { class: "apply-grid", id: "apply-body" });
  const ago = agoEl("read");
  root.replaceChildren(
    h(
      "p",
      { class: "measured" },
      "This plans every stack of the working copy against what the host last applied — not one stack's own Compare, which only checks the stack you already opened. Nothing runs until Apply… is confirmed; a stack whose directory is gone is destroyed only when its name is typed there.",
    ),
    h(
      "div",
      { class: "title-row" },
      h("span", { class: "actions-row" }, read, go),
    ),
    err,
    head,
    body,
    h("p", null, ago),
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
        "Reading every stack (latch is asked for the secrets the host's hash covers)…",
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
              " runs on the host; its directory is gone. Destroyed only when its name is typed in the apply form (backed up first, its data kept).",
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
    setAgo(ago, r.body.measured_at ?? Date.now() / 1000);
  };
  read.addEventListener("click", () => void load().catch(() => {}));
  go.addEventListener("click", () => void openAction("_host", "apply"));
  void load().catch(() => {});
  return () => abort.abort();
}
