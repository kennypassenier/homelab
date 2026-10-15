// A theme kp-themes no longer ships (high-contrast, sepia, retro and the
// earlier lapis / shade-*) can still sit in a browser's storage, and the
// boot script (chassis' theme-boot.js) has already written it on `<html>`.
// The picker would then show kp's fallback selected while storage kept
// saying otherwise. So a stored name kp does not know is replaced, on the
// page and in storage, by kp's own fallback, through kp's own calls. No
// theme is named here: what is known is whatever kp's list says.

import {
  DEFAULT_THEME,
  STORAGE_KEY,
  applyTheme,
  isTheme,
  storeTheme,
} from "/static/kp/js/theme-core.js";

/**
 * @param {Pick<Storage, "getItem"> | null} [storage] where the choice is
 *   kept (the browser's local storage)
 * @param {Element} [root] the element wearing the theme (`<html>`)
 * @returns {string | null} the fallback that was applied and stored; null
 *   when the stored theme is fine, or none is stored
 */
export function settleStoredTheme(
  storage = globalThis.localStorage ?? null,
  root,
) {
  /** @type {string | null} */
  let raw = null;
  try {
    raw = storage?.getItem(STORAGE_KEY) ?? null;
  } catch {
    return null;
  }
  if (raw == null || raw === "" || isTheme(raw)) return null;
  const next = applyTheme(DEFAULT_THEME, root ? { root } : {});
  storeTheme(
    next,
    storage ? { storage: /** @type {Storage} */ (storage) } : {},
  );
  return next;
}
