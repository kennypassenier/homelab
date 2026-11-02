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

/** The Undo toasts still on screen, so leaving the page can close them. */
const open = new Set();

/**
 * Close every Undo toast now. A page calls this from its stop: an Undo that
 * outlives its page would reverse a choice through controls that are gone.
 */
export function dismissUndos() {
  for (const close of [...open]) close();
  open.clear();
}

/**
 * Say what changed and offer Undo. The toast closes when its page stops
 * (`dismissUndos`).
 * @param {string} text what changed, in a sentence
 * @param {() => void} undo puts the choice back
 * @returns {() => void} closes the toast now
 */
export function offerUndo(text, undo) {
  /** @type {() => void} */
  let close = () => {};
  const run = () => {
    open.delete(close);
    undo();
  };
  close = toast(text, {
    ms: INSTANT_UNDO_MS,
    action: { label: "Undo", run, drive: { id: UNDO } },
  });
  open.add(close);
  setTimeout(() => open.delete(close), INSTANT_UNDO_MS + 1000);
  return close;
}
