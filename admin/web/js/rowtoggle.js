// Kenny, 2026-10-02: an expandable table row opens and closes when it is
// clicked anywhere, not only on the toggle in its first cell — except on
// a control of its own (a button, a link, a field), which keeps doing what
// it does. One delegated listener for every `data-kp-expandable` table in
// the dashboard, so no page wires it on its own and none can forget it.

/** What a click on these does is theirs, never the row's. */
const OWN_CONTROLS =
  "button, a, input, select, textarea, label, summary, [role=button], [contenteditable], [data-kp-row-toggle]";

/**
 * The toggle a click on `target` should press, or null when the click is
 * not the row's to take: outside an expandable table's body row, on a
 * detail row, or on a control of its own.
 * @param {Element | null} target
 * @returns {HTMLElement | null}
 */
export function rowToggleFor(target) {
  if (!target || target.closest(OWN_CONTROLS)) return null;
  const row = target.closest("[data-kp-expandable] tbody tr");
  if (!row) return null;
  return /** @type {HTMLElement | null} */ (
    row.querySelector(":scope > [data-kp-expand-cell] [data-kp-row-toggle]")
  );
}

/**
 * Whether the loaded kp-themes already opens a row from anywhere in it
 * (8.1.1 or newer, read from its `--kp-themes-version` token).
 * @param {string} version e.g. `"8.1.0"`, quotes allowed
 */
export function kpHandlesRowClicks(version) {
  const [maj, min, pat] = version
    .replace(/["'\s]/g, "")
    .split(".")
    .map((n) => Number.parseInt(n, 10) || 0);
  return maj > 8 || (maj === 8 && (min > 1 || (min === 1 && pat >= 1)));
}

/**
 * Installs the listener once, on `root` (the document by default).
 * @param {Document | HTMLElement} [root]
 */
export function attachRowToggle(root = document) {
  const version = getComputedStyle(document.documentElement).getPropertyValue(
    "--kp-themes-version",
  );
  if (kpHandlesRowClicks(version)) return;
  root.addEventListener("click", (event) => {
    // Selecting text in a row is reading it, not opening it.
    if (String(globalThis.getSelection?.() ?? "").length > 0) return;
    const toggle = rowToggleFor(/** @type {Element | null} */ (event.target));
    if (toggle) toggle.click();
  });
}
