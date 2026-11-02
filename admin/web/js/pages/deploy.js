// Deploy all changes, as a page of its own (ux-deploypage, Kenny, 2026-11-01:
// "Volledige pagina"; it was a 1200 px dialog on Stacks). `/stacks/deploy`
// holds the review and the run: the plan's steps, its datatable with a row
// per stack that opens to the exact lines, the sentence that says what Apply
// will do, and the same command for a workstation. Apply still ends on the
// apply dialog (apply.js), so the confirmation, the backup-first choice and
// the typed destroy are the ones every deploy has. The page has its own
// address, so the browser's Back returns to Stacks, a reload keeps the plan,
// and the plan gets the whole page width.
//
// The plan itself (apply.js, applyplan.js, applytable.js) is unchanged; this
// module owns what the dialog's caller used to: the header, the Compare step's
// button and note, and the command line.

import { labeledCliCommand } from "../actui.js";
import { fetchJson, h } from "../dom.js";
import { dialogControl } from "../drivable.js";
import { deployCount } from "../stacksview.js";
import { ensureStyle, pageHeader } from "../ui.js";
import { mount as mountPlan } from "./apply.js";

/** What `homelab apply` runs on a workstation: the whole plan, no prompt. */
export const APPLY_CLI = "homelab apply --yes";

/** @param {number} k */
const stacks = (k) => `${k} ${k === 1 ? "stack" : "stacks"}`;

/**
 * The Compare note's sentence.
 * @param {number | null} n how many stacks differ, null while unknown
 */
export function compareWords(n) {
  if (n == null) return "Comparing every stack with its files…";
  if (n === 0) return "Every stack matches its files.";
  return `${stacks(n)} ${n === 1 ? "differs" : "differ"} from ${n === 1 ? "its" : "their"} files, new and removed ones included.`;
}

/**
 * @param {HTMLElement} root
 * @param {{navigate: (href: string) => void}} _ctx
 * @returns {() => void}
 */
export function mount(root, _ctx) {
  ensureStyle("/css/pages/deploy.css");
  root.classList.add("dp-page");
  const abort = new AbortController();
  /** @type {any} */
  let drift = null;
  /** @type {{pending: number, broken: number} | null} */
  let planRead = null;

  const head = pageHeader({
    title: "Deploy all changes",
    desc: "Make every stack match its files in the repository, in one confirmed batch. Each stack is backed up before it changes; nothing runs until you press Apply.",
  });

  const note = h("p", { class: "dp-hint", "aria-live": "polite" });
  const button = dialogControl(
    h(
      "button",
      {
        type: "button",
        class: "kp-button kp-button--sm",
        id: "drift-compare",
        title:
          "Check every stack against its files again now and read the plan again",
      },
      "Compare again",
    ),
    "compare-now",
  );
  /** @type {Set<() => void>} */
  const subs = new Set();
  const count = () => (planRead ? planRead.pending : deployCount(drift));
  /** @type {string | null} */
  let failed = null;
  const paint = () => {
    note.textContent =
      failed != null && count() == null
        ? `The comparison with the files failed: ${failed}`
        : compareWords(count());
    subs.forEach((f) => f());
  };
  const compare = {
    button: h("div", { class: "ap-compare" }, button, note),
    at: () =>
      planRead != null || deployCount(drift) != null
        ? (drift?.measured_at ?? Date.now() / 1000)
        : null,
    /** @param {() => void} f */
    onChange: (f) => {
      subs.add(f);
      return () => subs.delete(f);
    },
  };

  /** @param {boolean} fresh */
  const readDrift = async (fresh) => {
    const r = await fetchJson(
      `/data/drift${fresh ? "?fresh=1" : ""}`,
      "the comparison with the files",
      abort.signal,
    );
    if (r.ok) drift = r.body;
    // A failed read must not leave "Comparing…" standing for ever.
    failed = r.ok ? null : r.error.why;
    paint();
  };
  button.addEventListener("click", () => {
    button.disabled = true;
    failed = null;
    note.textContent = compareWords(null);
    void readDrift(true)
      .catch(() => {})
      .finally(() => {
        button.disabled = false;
        paint();
      });
  });

  const planBody = h("div", { class: "dp-plan", id: "apply-section" });
  const cli = h(
    "div",
    { class: "act-cli act-cli--quiet dp-cli" },
    labeledCliCommand("The same from a workstation:", APPLY_CLI),
  );
  root.replaceChildren(head.el, planBody, cli);
  paint();
  const stopPlan = mountPlan(planBody, {
    compare,
    onRead: (read) => {
      planRead = read;
      paint();
      void readDrift(false).catch(() => {});
    },
  });
  void readDrift(false).catch(() => {});

  return () => {
    abort.abort();
    stopPlan();
    root.classList.remove("dp-page");
  };
}
