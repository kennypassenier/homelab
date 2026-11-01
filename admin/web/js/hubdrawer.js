// ux-stackhub-1: an edit that lives inline in a card (a secret's change
// form, the container's size and network form) opens in a side drawer
// instead, so pressing its button never scrolls the page away from the row
// that was pressed. The node is MOVED into the drawer (never copied), so
// every listener and every Live view handle it carries keeps working, and
// it goes back to the exact spot it came from when the drawer closes.

import { drawer } from "./ui.js";

/**
 * A drawer that borrows one node of the page while it is open.
 * @param {{node: HTMLElement, title: string, desc: string,
 *   cls?: string, onClose?: () => void}} spec
 * @returns {{open: () => void, close: () => void, isOpen: () => boolean,
 *   setHead: (title: string, desc: string) => void,
 *   forget: () => void}}
 */
export function nodeDrawer(spec) {
  /** @type {ReturnType<typeof drawer> | null} */
  let panel = null;
  /** Where the node lives while the drawer is shut. */
  const home = document.createComment("drawer-home");
  let gone = false;

  const giveBack = () => {
    if (home.isConnected) home.replaceWith(spec.node);
  };
  /** @param {string} title @param {string} desc */
  const setHead = (title, desc) => {
    const el = panel?.el;
    const t = el?.querySelector(".kp-dialog__title");
    const d = el?.querySelector(".kp-drawer__desc");
    if (t) t.textContent = title;
    if (d) d.textContent = desc;
  };
  return {
    open: () => {
      if (panel) return;
      spec.node.replaceWith(home);
      const made = drawer({
        title: spec.title,
        desc: spec.desc,
        cls: spec.cls ?? "hub-drawer",
        body: spec.node,
        onClose: () => {
          if (gone) return;
          giveBack();
          panel = null;
          spec.onClose?.();
        },
      });
      panel = made;
      made.open();
    },
    close: () => panel?.close(),
    isOpen: () => panel != null,
    setHead,
    // The page that owned the node was replaced: the drawer goes, the
    // placeholder with it, and the (stale) node is dropped.
    forget: () => {
      gone = true;
      const p = panel;
      panel = null;
      if (p?.el.open) p.el.close();
      p?.el.remove();
      home.remove();
    },
  };
}
