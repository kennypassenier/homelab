// fix-239: this page load's own name, for claiming a page-control step: of
// several tabs that follow, exactly one clicks (the server lets the first
// claim win, and keeps a page-level dialog's later steps with it).
//
// review (the unexplained 15 s no-answer): the name the same browser tab
// had before it reloaded is kept in sessionStorage (it survives a reload,
// never another tab) and reported with the new one on attach, so the
// dashboard lets go of a page-level dialog the tab held before reloading:
// a reload closes every dialog, and every later claim was refused as "open
// in another tab".

const KEY = "homelab.drive.tab";

/** sessionStorage, or null in a private window or a test. */
function storage() {
  try {
    return globalThis.sessionStorage ?? null;
  } catch {
    return null;
  }
}

/** This page load's name. */
export const TAB =
  globalThis.crypto?.randomUUID?.() ??
  `tab-${Date.now()}-${Math.random().toString(36).slice(2)}`;

/** The name this browser tab had before it reloaded, if it had one. */
export const WAS_TAB = (() => {
  const s = storage();
  try {
    const was = s?.getItem(KEY) ?? null;
    s?.setItem(KEY, TAB);
    return was;
  } catch {
    return null;
  }
})();
