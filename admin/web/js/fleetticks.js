// The ticked stacks of the Stacks list: `ticked` follows the boxes a person
// clicks, and a repaint ticks the boxes of the keys in it again.

/** @typedef {import("./fleetpaint.js").PaintEnv} PaintEnv */

/**
 * Tick the boxes of the keys in `ticked`, the way a click would.
 * @param {PaintEnv} env
 */
export function reapplyTicks({ body, st }) {
  for (const box of /** @type {NodeListOf<HTMLInputElement>} */ (
    body.querySelectorAll("input[data-kp-select-row]")
  )) {
    const key = box
      .closest("[data-kp-row-key]")
      ?.getAttribute("data-kp-row-key");
    const want = !!key && st.ticked.has(key) && !box.disabled;
    if (box.checked !== want) {
      box.checked = want;
      box.dispatchEvent(new Event("change", { bubbles: true }));
    }
  }
}

/** Keep `ticked` in step with the boxes a person clicks. @param {PaintEnv} env */
export function wireTicks({ body, st }) {
  body.addEventListener("change", (e) => {
    const box = e.target;
    if (
      !(box instanceof HTMLInputElement) ||
      !box.matches("[data-kp-select-row]")
    )
      return;
    const key = box
      .closest("[data-kp-row-key]")
      ?.getAttribute("data-kp-row-key");
    if (!key) return;
    if (box.checked) st.ticked.add(key);
    else st.ticked.delete(key);
  });
}
