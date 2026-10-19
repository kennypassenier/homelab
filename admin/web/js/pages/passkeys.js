// feat-pages-1 (chassis-rs 3.1.0, nav-decisions): the kit's own Passkeys.
// redesign-settings (3.71.0): the passkeys are the left half of Settings'
// Sign-in card, beside the machine tokens (signin.js draws both on
// kp-themes' passkey list). `/settings?section=sign-in` (where `/passkeys`
// goes, router.js) mounts this module (main.js VIEWS), which draws the whole
// Settings page opened at Sign-in.

import { mount as mountSettings } from "./settings.js";

/**
 * `/settings?section=sign-in`: the Settings page, opened at Sign-in.
 * @param {HTMLElement} root
 * @returns {() => void}
 */
export function mount(root) {
  return mountSettings(root, { section: "sign-in" });
}
