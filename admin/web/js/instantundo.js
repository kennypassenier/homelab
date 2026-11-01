// ux-hostsettings-2: a choice that applies the moment it is made (This
// browser, Account) says what changed and offers Undo, the way a Save-gated
// host.toml key goes back from the Save bar. The change is already done;
// Undo calls the reverse the caller hands in.

import { declare } from "./drivable.js";
import { toast } from "./ui.js";

const UNDO = declare({
  id: "settings-instant-undo",
  page: "settings",
  opens: "run",
  what: "take back the choice just made in This browser or Account",
  shows: "for a few seconds after such a choice",
});

/** How long the Undo is offered, in milliseconds. */
export const INSTANT_UNDO_MS = 6000;

/**
 * Say what changed and offer Undo.
 * @param {string} text what changed, in a sentence
 * @param {() => void} undo puts the choice back
 * @returns {() => void} closes the toast now
 */
export function offerUndo(text, undo) {
  return toast(text, {
    ms: INSTANT_UNDO_MS,
    action: { label: "Undo", run: undo, drive: { id: UNDO } },
  });
}
