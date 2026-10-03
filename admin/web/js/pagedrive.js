// fix-239: the tab's own half of a page-control step (`homelab ui click`,
// and typing, ticking and pressing inside the page-level dialog a click
// opened). The dashboard's server cannot model these dialogs — a stale
// image's Update runs its backup, commit and deploy from the tab itself —
// so ONE tab that follows (the one the server let claim the step) finds the
// control the page declared (drivable.js), clicks it as a person would, and
// answers what happened: the page it is on, the dialog now open, or why it
// could not. The server hands that answer to `homelab ui`.

import { control, pick } from "./drivable.js";
import { pageHref, shownPage } from "./router.js";

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
 * Look for the control for up to `ms` (a page draws its rows after its
 * read answers).
 * @param {() => ParentNode | null} rootOf
 * @param {boolean} inDialog
 * @param {{id: string, row: string | null}} want
 * @param {number} ms
 */
async function find(rootOf, inDialog, want, ms) {
  const end = Date.now() + ms;
  for (;;) {
    const root = rootOf();
    const els = root ? candidates(root, inDialog) : [];
    const r = pick(want, els.map(described));
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
    } else if (Date.now() > end) return { el: null, why: r.why, fix: r.fix };
    await sleep(100);
  }
}

/**
 * @param {Partial<TabAnswer>} a
 * @returns {TabAnswer}
 */
const answer = (a) => ({
  ok: false,
  page: location.pathname,
  dialog: titleOf(topDialog()),
  ...a,
});

/**
 * Click a control: in the dialog on top when one is open, else on the
 * page — the control's own page first when it is not on this one.
 * @param {string} id
 * @param {string | null} row
 * @param {(href: string) => void} navigate
 * @param {(el: HTMLElement) => Promise<void>} show
 * @returns {Promise<TabAnswer>}
 */
async function click(id, row, navigate, show) {
  const want = { id, row };
  const d = topDialog();
  if (d) {
    const f = await find(() => d, true, want, 1500);
    if (!f.el) return answer({ why: `${f.why} in ${titleOf(d)}`, fix: f.fix });
    return press(f.el, show, false);
  }
  const page = () => document.getElementById("page");
  const c = control(id);
  const home = c ? (c.at?.(row) ?? pageHref(c.page)) : null;
  // feat-shell-1: a pre-3.71.0 module shown as a view of its new home
  // (`/activity?view=planned` is Schedules) counts as that module's page.
  const here = shownPage(location.pathname, location.search);
  // Declared on another page: go there first, as a person would.
  const there = c?.at
    ? location.pathname + location.search === home
    : here === c?.page;
  if (c && home && !there) {
    navigate(home);
    await sleep(250);
  }
  const f = await find(page, false, want, c ? 8000 : 1500);
  if (!f.el)
    return answer({
      why: f.why,
      fix: c ? f.fix : `${f.fix}; no page declares a control ${id}`,
    });
  return press(f.el, show, c?.opens === "dialog");
}

/**
 * @param {HTMLElement} el
 * @param {(el: HTMLElement) => Promise<void>} show
 * @param {boolean} opensDialog
 * @returns {Promise<TabAnswer>}
 */
async function press(el, show, opensDialog) {
  const before = topDialog();
  await show(el);
  el.click();
  // A dialog that reads first draws its title at once; wait for it.
  if (opensDialog) {
    const end = Date.now() + 3000;
    while (topDialog() === before && Date.now() < end) await sleep(50);
  } else await sleep(150);
  return answer({ ok: true });
}

/**
 * The field of the dialog on top, else of the page, by its id or its name.
 * @param {string} field
 */
function fieldOf(field) {
  const d = topDialog() ?? document.getElementById("page");
  if (!d) return { d, el: null };
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
 * @returns {Promise<TabAnswer>}
 */
export async function takeStep(step, navigate, show) {
  switch (step.do) {
    case "click":
      return click(step.control ?? "", step.row ?? null, navigate, show);
    case "press":
      if (!topDialog())
        return answer({
          why: "no dialog is open",
          fix: "homelab ui click the control that opens it first",
        });
      return click(step.button ?? "", null, navigate, show);
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
