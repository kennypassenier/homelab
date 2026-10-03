// fix-239: the tab's own half of a page-control step (`homelab ui click`,
// and typing, ticking and pressing inside the page-level dialog a click
// opened). The dashboard's server cannot model these dialogs — a stale
// image's Update runs its backup, commit and deploy from the tab itself —
// so ONE tab that follows (the one the server let claim the step) finds the
// control the page declared (drivable.js), clicks it as a person would, and
// answers what happened: the page it is on, the dialog now open, or why it
// could not. The server hands that answer to `homelab ui`.

import {
  clickLine,
  closest,
  currentField,
  declaredField,
  fields as declaredFields,
  howToReach,
  pick,
  resolve,
} from "./drivable.js";
import { pageHref, shownPage } from "./router.js";

/**
 * review (the unexplained 15 s no-answer): the longest a tab spends on one
 * page-control step — going to the control's page, looking for it, waiting
 * for the dialog it opens — in ms. One budget for the whole step, below the
 * dashboard's own wait for the answer (`TAB_WAIT`, 15 s) with room for the
 * claim's and the answer's round trips; the generated catalog carries it
 * and an admin test holds the two apart.
 */
export const TAB_BUDGET_MS = 11000;

/**
 * The clock of one step: its deadline, and whether the dashboard closed it
 * (it gave up waiting: the tab stops at once rather than click late).
 * @typedef {{end: number, cancelled: () => boolean}} Budget
 */

/** @returns {Budget} */
const freshBudget = () => ({
  end: Date.now() + TAB_BUDGET_MS,
  cancelled: () => false,
});

/** @param {Budget} b */
const left = (b) => Math.max(0, b.end - Date.now());

/**
 * A page-control step as the server sends it.
 * @typedef {{do: string, control?: string, row?: string | null,
 *   field?: string, text?: string, value?: string, on?: boolean,
 *   button?: string}} TabStep
 * What the tab answers.
 * @typedef {{ok: boolean, why?: string, fix?: string, page: string,
 *   dialog: string | null}} TabAnswer
 */

/** @param {number} ms */
const sleep = (ms) => new Promise((r) => setTimeout(r, ms));

/** @param {Element} el */
const visible = (el) => el.getClientRects().length > 0;

/**
 * The dialog on top of the page, if one is open.
 * @returns {HTMLDialogElement | null}
 */
export function topDialog() {
  const open = [...document.querySelectorAll("dialog[open]")];
  return /** @type {HTMLDialogElement | null} */ (open.at(-1) ?? null);
}

/** @param {HTMLDialogElement | null} d */
const titleOf = (d) =>
  d
    ? (d.querySelector(".kp-dialog__title")?.textContent ?? "").trim() ||
      "a dialog"
    : null;

/**
 * The clickable things in `root`: on a page only the marked controls; in
 * a dialog every button, marked or found by its label.
 * @param {ParentNode} root
 * @param {boolean} inDialog
 * @returns {HTMLElement[]}
 */
function candidates(root, inDialog) {
  const sel = inDialog
    ? "[data-drive], button, input[type=checkbox]"
    : "[data-drive]";
  return [...root.querySelectorAll(sel)]
    .map((x) => /** @type {HTMLElement} */ (x))
    .filter(visible);
}

/** @param {HTMLElement} el */
const described = (el) => ({
  id: el.dataset.drive ?? "",
  row: el.dataset.driveRow ?? null,
  label: (el.getAttribute("aria-label") ?? el.textContent ?? "").trim(),
});

/**
 * drive-reach: the page has drawn: a title, and no skeleton left.
 */
function drawn() {
  const p = document.getElementById("page");
  return (
    !!p?.querySelector("h1, h2") &&
    ![...p.querySelectorAll(".kp-skeleton")].some(
      (e) => e.getClientRects().length > 0,
    )
  );
}

/**
 * Look for the control for up to `ms` (a page draws its rows after its
 * read answers). With `settle`, a control not on screen at all is given up
 * on once the page has stood drawn that long: a control that shows only in
 * a state is not waited for the whole `ms`.
 * @param {() => ParentNode | null} rootOf
 * @param {boolean} inDialog
 * @param {{id: string, row: string | null}} want
 * @param {number} ms
 * @param {Budget} budget
 * @param {number} [settle]
 * @returns {Promise<{el: HTMLElement | null, why: string, fix: string}>}
 */
async function find(rootOf, inDialog, want, ms, budget, settle = 0) {
  const end = Date.now() + Math.min(ms, left(budget));
  let calm = 0;
  for (;;) {
    if (budget.cancelled())
      return {
        el: null,
        why: "the dashboard stopped waiting for this step",
        fix: "send it again",
      };
    const root = rootOf();
    const els = root ? candidates(root, inDialog) : [];
    const r = pick(want, els.map(described), inDialog);
    if ("index" in r) {
      const el = els[r.index];
      if (!(el instanceof HTMLButtonElement && el.disabled))
        return { el, why: "", fix: "" };
      if (Date.now() > end)
        return {
          el: null,
          why: `${want.id} is on screen but disabled`,
          fix: "the page holds it until what it needs is there; homelab ui state, then try again",
        };
    } else {
      const gone = settle > 0 && r.why.startsWith("there is no control");
      calm = gone && drawn() ? calm || Date.now() : 0;
      if (Date.now() > end || (calm && Date.now() - calm >= settle))
        return { el: null, why: r.why, fix: r.fix };
    }
    await sleep(100);
  }
}

/**
 * @param {Partial<TabAnswer>} a
 * @returns {TabAnswer}
 */
const answer = (a) => {
  const d = topDialog();
  return {
    ok: false,
    page: location.pathname,
    dialog: titleOf(d),
    // review H3/M5: what the dialog on top offers, so the client and the
    // dashboard check the next step against it before any tab is asked.
    ...(d
      ? {
          controls: candidates(d, true)
            .filter((e) => !(e instanceof HTMLInputElement))
            .map(described)
            .map(({ id, label }) => ({ id, label })),
          fields: [...d.querySelectorAll("input, select, textarea")]
            .map((x) => x.id || x.getAttribute("name") || "")
            .filter(Boolean),
        }
      : {}),
    ...a,
  };
};

/**
 * drive-reach: where a declared control lives now: its own address when it
 * lives per stack (`at`), else its page's, else its page's old address,
 * which the router sends on to the page's new home (one redirect table for
 * the browser and Live view).
 * @param {import("./drivable.js").Control} c
 * @param {string | null} row
 */
export const homeOf = (c, row) =>
  c.at?.(row) ?? pageHref(c.page) ?? `/${c.page}`;

/**
 * drive-reach: the refusal for a name no page declares, ever: the closest
 * declared controls, where each lives, and the line that clicks it.
 * @param {string} id
 * @returns {TabAnswer}
 */
function unknownControl(id) {
  const near = closest(id, 4);
  return answer({
    why: `no page declares a control ${id}`,
    fix: near.length
      ? `the closest: ${near
          .map((c) => `${c.id} (on ${c.page}: ${c.what})`)
          .join(
            "; ",
          )}; e.g. ${clickLine(near[0])}; homelab ui list <words> lists every control`
      : "homelab ui list lists every control, its page and the line that clicks it",
  });
}

/**
 * Click a control: in the dialog on top when one is open, else on the
 * page: on screen now when it is (its module may be drawn inside another
 * page), else on the control's own page, gone to first, as a person would.
 * An old name (`was`) clicks the control it became; when it became an item
 * of that control's menu, the dashboard sends the press as a step of its
 * own (review: one step, one budget).
 * @param {string} id
 * @param {string | null} row
 * @param {(href: string) => void} navigate
 * @param {(el: HTMLElement) => Promise<void>} show
 * @param {Budget} budget
 * @returns {Promise<TabAnswer>}
 */
async function click(id, row, navigate, show, budget) {
  const d = topDialog();
  if (d) {
    // redesign-integrate-8: a page drawn as a dialog (Deploy all changes
    // on Stacks) draws its declared controls after its own read; wait for
    // one as long as for a page's, a dialog's plain buttons as before.
    const f = await find(
      () => d,
      true,
      { id, row },
      resolve(id) ? 8000 : 1500,
      budget,
    );
    if (!f.el) return answer({ why: `${f.why} in ${titleOf(d)}`, fix: f.fix });
    return press(f.el, show, false, budget);
  }
  // drive-reach: a page's toasts (an Undo) sit at the screen's edge, outside
  // #page; declared controls are found wherever the page drew them.
  const page = () => (document.getElementById("page") ? document.body : null);
  const hit = resolve(id);
  // review H3: a page's own control answers to its declared id only (the
  // client and the dashboard refuse an undeclared one before a tab sees it).
  if (!hit) return unknownControl(id);
  const c = hit.control;
  const want = { id: c.id, row };
  let f = await find(page, false, want, 0, budget);
  // Only a control that is not on screen at all is gone looking for: one
  // that is (disabled, or on rows a step must name) is answered here.
  if (!f.el && f.why.startsWith("there is no control")) {
    const home = homeOf(c, row);
    // feat-shell-1: a pre-3.71.0 module shown as a view of its new home
    // (`/activity?view=planned` is Schedules) counts as that module's page.
    const here = shownPage(location.pathname, location.search);
    // A module drawn inside another page (a stack hub's Settings shows the
    // secrets module) counts as its page when its own controls are there.
    const moduleHere = [...document.querySelectorAll("[data-drive]")].some(
      (e) =>
        visible(e) &&
        resolve(/** @type {HTMLElement} */ (e).dataset.drive ?? "")?.control
          .page === c.page,
    );
    const there = c.at
      ? location.pathname + location.search === home
      : here === c.page || moduleHere;
    if (!there) {
      if (budget.cancelled())
        return answer({ why: "the dashboard stopped waiting for this step" });
      navigate(home);
      await sleep(250);
    }
    f = await find(page, false, want, 8000, budget, 2500);
  }
  if (!f.el) {
    const at = `${location.pathname}${location.search}`;
    const missing = f.why.startsWith("there is no control");
    const disabled = f.why.endsWith("disabled");
    return answer({
      why: missing
        ? `${c.id} is not on screen at ${at}${c.shows ? `: it shows only ${c.shows}` : ""}`
        : disabled && c.shows
          ? `${f.why}: it works only ${c.shows}`
          : f.why,
      fix: missing
        ? `${c.reach?.length ? "reach it" : "it lives on " + c.page}: ${howToReach(c, row)}`
        : f.fix,
    });
  }
  return press(f.el, show, c.opens === "dialog" || hit.press != null, budget);
}

/**
 * @param {HTMLElement} el
 * @param {(el: HTMLElement) => Promise<void>} show
 * @param {boolean} opensDialog
 * @param {Budget} budget
 * @returns {Promise<TabAnswer>}
 */
async function press(el, show, opensDialog, budget) {
  const before = topDialog();
  await show(el);
  const href = location.href;
  if (budget.cancelled())
    return answer({
      why: "the dashboard stopped waiting for this step",
      fix: "send it again",
    });
  // redesign-integrate-8: an SVG control (the Map's nodes) has no
  // click(); it took the step and never answered.
  if (typeof el.click === "function") el.click();
  else
    el.dispatchEvent(
      new MouseEvent("click", {
        bubbles: true,
        cancelable: true,
        view: window,
      }),
    );
  // A person's click on a text box puts the caret in it; click() alone
  // does not (a search box's press did nothing).
  if (el.matches("input:not([type=checkbox]):not([type=radio]), textarea"))
    el.focus({ preventScroll: true });
  // A dialog that reads first draws its title at once; wait for it, within
  // the step's budget.
  // redesign-openpoints-1: a press that went to another address (a control
  // whose rows differ: the hub header's Update is the Update flow, its Back
  // up and Deploy are dialogs) opens no dialog; it answers at once.
  if (opensDialog) {
    const end = Date.now() + Math.min(3000, left(budget));
    while (topDialog() === before && location.href === href && Date.now() < end)
      await sleep(50);
    if (location.href !== href) await sleep(150);
  } else await sleep(150);
  return answer({ ok: true });
}

/**
 * The field of the dialog on top, else of the page, by its id or its name.
 * @param {string} field
 */
function fieldOf(field) {
  // drive-reach: a field a redesign renamed answers to its old id too.
  field = currentField(field);
  const dialog = topDialog();
  const d = dialog ?? document.getElementById("page");
  if (!d) return { d, el: null };
  // review M5: on the page itself only a declared field is set.
  if (!dialog && !declaredField(field)) return { d, el: null };
  const el =
    d.querySelector(`#${CSS.escape(field)}`) ??
    d.querySelector(`[name="${CSS.escape(field)}"]`);
  return { d, el };
}

/** @param {HTMLElement} d */
const fieldIds = (d) =>
  [...d.querySelectorAll("input, select, textarea")]
    .map((x) => x.id || x.getAttribute("name") || "")
    .filter(Boolean)
    .join(", ") || "none";

/**
 * @param {string} field
 * @param {string | boolean} value
 * @param {(el: HTMLElement) => Promise<void>} show
 * @returns {Promise<TabAnswer>}
 */
async function setField(field, value, show) {
  const { d, el } = fieldOf(field);
  if (!d)
    return answer({
      why: "no page is on screen",
      fix: "homelab ui goto the page first",
    });
  const where = d instanceof HTMLDialogElement ? titleOf(d) : "this page";
  if (!(d instanceof HTMLDialogElement) && !declaredField(field))
    return answer({
      why: `no page declares a field ${field}`,
      fix: `the page fields are: ${declaredFields()
        .map((f) => (f.row ? `${f.id}-${f.row}` : f.id))
        .join(", ")}`,
    });
  if (!(
    el instanceof HTMLInputElement ||
    el instanceof HTMLSelectElement ||
    el instanceof HTMLTextAreaElement
  ))
    return answer({
      why: `${where} has no field ${field}`,
      fix: `its fields are: ${fieldIds(d)}`,
    });
  await show(/** @type {HTMLElement} */ (el.closest(".kp-field") ?? el));
  if (el instanceof HTMLInputElement && el.type === "checkbox") {
    if (typeof value !== "boolean")
      return answer({
        why: `${field} is a check field`,
        fix: `homelab ui check ${field} on|off`,
      });
    if (el.checked !== value) el.click();
    return answer({ ok: true });
  }
  const text = String(value);
  if (
    el instanceof HTMLSelectElement &&
    ![...el.options].some((o) => o.value === text)
  )
    return answer({
      why: `${text} is not a choice of ${field}`,
      fix: `its choices are: ${[...el.options].map((o) => o.value).join(", ")}`,
    });
  el.value = text;
  el.dispatchEvent(new Event("input", { bubbles: true }));
  el.dispatchEvent(new Event("change", { bubbles: true }));
  return answer({ ok: true });
}

/**
 * Take one page-control step in this tab.
 * @param {TabStep} step
 * @param {(href: string) => void} navigate
 * @param {(el: HTMLElement) => Promise<void>} show marks the element
 *   before it is used (the press flash, the cursor)
 * @param {() => boolean} [cancelled] true once the dashboard closed this
 *   step (it stopped waiting): the tab stops at once
 * @returns {Promise<TabAnswer>}
 */
export async function takeStep(step, navigate, show, cancelled) {
  const budget = freshBudget();
  if (cancelled) budget.cancelled = cancelled;
  switch (step.do) {
    case "click":
      return click(
        step.control ?? "",
        step.row ?? null,
        navigate,
        show,
        budget,
      );
    case "press":
      if (!topDialog())
        return answer({
          why: "no dialog is open",
          fix: "homelab ui click the control that opens it first",
        });
      return click(step.button ?? "", null, navigate, show, budget);
    case "type":
    case "edit":
      return setField(step.field ?? "", step.text ?? "", show);
    case "pick":
      return setField(step.field ?? "", step.value ?? "", show);
    case "check":
      return setField(step.field ?? "", step.on === true, show);
    default:
      return answer({
        why: `ui ${step.do} is not a page-control step`,
        fix: "homelab ui state",
      });
  }
}
