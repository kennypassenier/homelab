// The kp-themes module chassis-rs serves at /static/kp/js/overlays.js (kp-themes 8.0.0),
// copied from kp-themes js/overlays.d.ts: chassis serves the module, not its types.
/** How long a toast stays before it removes itself. An operational knob; per toast as `ms`. */
export declare const TOAST_MS = 5000;
/** Dispatched on the dialog, bubbling, when a wired trigger opened it: `{ trigger, modal }`. */
export declare const DIALOG_OPEN_EVENT = "kp-dialog-open";
/** Dispatched on a tab list, bubbling, when the selected tab changed: `{ index, tab, panel, previous }`. */
export declare const TAB_CHANGE_EVENT = "kp-tab-change";
/** Dispatched on the toast region when a toast was shown / removed: `{ toast, text }`. */
export declare const TOAST_SHOW_EVENT = "kp-toast-show";
export declare const TOAST_HIDE_EVENT = "kp-toast-hide";
/**
 * Put a dialog that has just opened, and its `.kp-dialog__body`, back at
 * the top [scope-96]. A closed dialog keeps its scroll position, so a
 * long dialog scrolled and closed reopened where it was left (Kenny,
 * 2026-09-15: "Bovenaan openen"). Called after showModal()/show(), because
 * a closed dialog has no box to scroll. The one exception is focus: when
 * the element the dialog focused on opening lies below the fold, it is
 * scrolled back into view, as far as needed and no further, so focus is
 * never hidden.
 *
 * @param {HTMLDialogElement} dialog
 */
export declare function openAtTop(dialog: HTMLDialogElement): void;
/**
 * Wire `[data-kp-dialog="<id>"]` buttons to the dialog with that id.
 *
 * showModal() by default: the modal form is the one that traps focus and
 * makes the rest of the page inert. `data-kp-dialog-mode="non-modal"` on
 * the trigger opens with show() instead, for a palette-like panel that
 * must not inert the page.
 *
 * @param {ParentNode} root
 * @param {{ modal?: boolean }} [options] the default mode when a trigger says nothing
 * @returns {() => void} detach
 */
export declare function attachDialogs(
  root?: ParentNode,
  {
    modal,
  }?: {
    modal?: boolean;
  },
): () => void;
/**
 * Select a tab in an attached tab list from outside — for a "next" button,
 * a URL hash, or a restored view.
 *
 * @param {Element} list the `[role="tablist"]`
 * @param {number | string} which an index, or a tab's id
 */
export declare function selectTab(list: Element, which: number | string): void;
/**
 * Scroll a tab row, and only the row, until `tab` is inside it [gap-12].
 *
 * A row longer than its box scrolls (css/components.css), and a selected
 * tab scrolled out of sight is a selection nobody can see. Not
 * `scrollIntoView`: that scrolls every ancestor too, and a tab changed from
 * a "next" button would pull the whole page to the row.
 *
 * @param {Element} list the `[role="tablist"]`
 * @param {Element | undefined} tab
 */
export declare function revealTab(
  list: Element,
  tab: Element | undefined,
): void;
/** Written on a tab row while its tabs do not fit, which is what makes it scroll [gap-12]. */
export declare const TABS_OVERFLOW = "data-kp-tabs-overflow";
/**
 * Keep `data-kp-tabs-overflow` on a tab row exactly while its tabs are
 * wider than the row [gap-12]. The stylesheet scrolls only such a row, so a
 * row that fits keeps every pixel a register draws past its edge.
 *
 * @param {HTMLElement} list the `[role="tablist"]`
 * @param {() => Element | undefined} selected the tab to keep in view when the row starts to scroll
 * @returns {() => void} stop, which also takes the attribute away
 */
export declare function watchTabOverflow(
  list: HTMLElement,
  selected: () => Element | undefined,
): () => void;
/**
 * Tabs: one stop in the tab order, arrows to move between them.
 *
 * This is the ARIA authoring practice, and it is genuinely not free: a
 * tab list where every tab is a tab stop makes a keyboard user press Tab
 * seven times to leave a row of tabs.
 *
 * Options, each also readable from the list's own attributes so a
 * server-written page needs no JavaScript to set them:
 *   - `aria-orientation="vertical"` → Up/Down move instead of Left/Right
 *   - `data-kp-activation="manual"` → arrows move focus only; Enter or
 *     Space selects. The other half of the ARIA practice, for a panel
 *     that is expensive to show.
 *   - `data-kp-loop="false"` → no wrap-around at the ends
 *
 * @param {ParentNode} root
 * @param {{ activation?: 'automatic' | 'manual', loop?: boolean }} [options] defaults for lists that say nothing
 * @returns {() => void} detach
 */
export declare function attachTabs(
  root?: ParentNode,
  {
    activation,
    loop,
  }?: {
    activation?: "automatic" | "manual";
    loop?: boolean;
  },
): () => void;
/**
 * The page's toast region, created on first use.
 *
 * @param {{ region?: HTMLElement | null, role?: string, live?: 'polite' | 'assertive' | 'off', className?: string }} [options]
 * @returns {HTMLElement}
 */
export declare function toastRegion({
  region,
  role,
  live,
  className,
}?: {
  region?: HTMLElement | null;
  role?: string;
  live?: "polite" | "assertive" | "off";
  className?: string;
}): HTMLElement;
/**
 * Show a toast in the page's toast region, creating the region if it is
 * not there. role="status" rather than role="alert" by default: a toast
 * is an announcement, and alert interrupts whatever a screen reader was
 * saying. An error that must interrupt passes `live: 'assertive'` on its
 * own toast, which gets its own live region so the politeness is per
 * message rather than per page.
 *
 * @param {string | Node} content text, or a node the consumer built
 * @param {{ ms?: number, region?: HTMLElement | null, live?: 'polite' | 'assertive', className?: string, action?: { label: string, onClick: () => void }, max?: number }} [options]
 *   ms: 0 keeps the toast until dismissed; max: drop the oldest beyond this many
 * @returns {HTMLElement & { dismiss: () => void }} the toast, so a caller can remove it early
 */
export declare function toast(
  content: string | Node,
  {
    ms,
    region,
    live,
    className,
    action,
    max,
  }?: {
    ms?: number;
    region?: HTMLElement | null;
    live?: "polite" | "assertive";
    className?: string;
    action?: {
      label: string;
      onClick: () => void;
    };
    max?: number;
  },
): HTMLElement & {
  dismiss: () => void;
};
/** Dispatched on an alert, bubbling and cancelable, before its close button hides it: `{ alert, button }`. */
export declare const ALERT_DISMISS_EVENT = "kp-alert-dismiss";
/** Close buttons another channel wires itself; the React Alert and Toasts mark theirs [AR29]. */
export declare const DISMISS_OWNED = "[data-kp-dismiss-owner]";
/**
 * Make `.kp-alert__close` and `.kp-toast__close` close what they sit in,
 * without a framework [gap-11].
 *
 * Before this only the React components did anything with those buttons;
 * a server-rendered alert kept a close button that did nothing, which is
 * worse than no button at all. Delegated from `root`, so a toast raised
 * after attach is covered too.
 *
 * An alert is hidden (`hidden`, which the base layer holds above every
 * layout class) after ALERT_DISMISS_EVENT, which a consumer may cancel to
 * keep it or to animate it out first — setting `hidden = false` brings it
 * back. A toast leaves through its own `dismiss()` when `toast()` made it,
 * so TOAST_HIDE_EVENT fires as it does on a timeout; otherwise it is
 * removed and the same event is dispatched on its region.
 *
 * @param {ParentNode} root
 * @param {{ ownedBy?: string }} [options] `ownedBy: ''` wires even the buttons another channel marked
 * @returns {() => void} detach
 */
export declare function attachDismissals(
  root?: ParentNode,
  {
    ownedBy,
  }?: {
    ownedBy?: string;
  },
): () => void;
/** Dispatched on a tooltip anchor, bubbling, when its tooltip opens or closes: `{ open, tooltip }`. */
export declare const TOOLTIP_EVENT = "kp-tooltip";
/** Tooltip anchors another channel wires itself; the React Tooltip marks its own [AR29]. */
export declare const TOOLTIP_OWNED = "[data-kp-tooltip-owner]";
/**
 * Framework-free tooltips on `.kp-tooltip-anchor` [gap-11].
 *
 *   <span class="kp-tooltip-anchor">
 *     <button type="button" class="kp-button">Recalibrate</button>
 *     <span role="tooltip" class="kp-popover kp-tooltip">Takes about two minutes</span>
 *   </span>
 *
 * The same contract as the React Tooltip: hidden at rest, shown after a
 * short delay under the pointer and at once on focus, gone when the
 * pointer or focus leaves, and gone on Escape (WCAG 1.4.13). The trigger
 * is described by the tooltip — an id is given where there was none, and
 * `aria-describedby` gains it rather than losing what it held. The anchor
 * names itself for anchor positioning, as the React channel does. Detach
 * puts back the attributes, the styles and the hidden state it found.
 *
 * @param {ParentNode} root
 * @param {{ openDelayMs?: number, closeDelayMs?: number, closeOnEscape?: boolean, ownedBy?: string }} [options]
 *   Per anchor: `data-kp-open-delay`, `data-kp-close-delay`, `data-kp-close-on-escape="false"`.
 * @returns {() => void} detach
 */
export declare function attachTooltips(
  root?: ParentNode,
  {
    openDelayMs,
    closeDelayMs,
    closeOnEscape,
    ownedBy,
  }?: {
    openDelayMs?: number;
    closeDelayMs?: number;
    closeOnEscape?: boolean;
    ownedBy?: string;
  },
): () => void;
/** Written on an overlay's scroll box while its content is taller than the box. */
export declare const SCROLL_OVERFLOW = "data-kp-popover-overflowing";
/** The boxes `attachScrollbars` watches: the popover (a menu, a tooltip), a dialog, a dialog's body. */
export declare const SCROLL_BOXES = ".kp-popover, .kp-dialog, .kp-dialog__body";
/**
 * Tell a register whether an overlay's box scrolls, and where [retro notes, 2026-09-15].
 *
 * Theme-neutral data, nothing drawn: `data-kp-popover-overflowing` exactly while the
 * content is taller than the box, and three custom properties while it is —
 * `--kp-scroll-view` (the box's inner height in px), `--kp-scroll-ratio`
 * (the part of the content in view, 0–1) and `--kp-scroll-progress` (how far
 * it is scrolled, 0–1). A register that draws its own scrollbar reads them;
 * every other theme never notices them.
 *
 * Such a register also declares `--kp-scrollbar-size` (the bar's width),
 * `--kp-scrollbar-inset` (its distance from the padding box's end edge and
 * from the top and bottom) and `--kp-scrollbar-button` (an arrow button's
 * height) on the box. Only then does a press on the drawn bar do what a
 * platform scrollbar's does: an arrow scrolls a line, the track a page,
 * and the thumb is dragged. A press on a bar with nothing to scroll does
 * nothing. The bar is drawn at the inline end of a left-to-right box.
 *
 * @param {HTMLElement} box
 * @returns {() => void} stop, which also takes the attribute and the properties away
 */
export declare function watchScrollbar(box: HTMLElement): () => void;
/**
 * `watchScrollbar` on every `.kp-popover`, `.kp-dialog` and
 * `.kp-dialog__body` under `root`, including the ones added later (a theme
 * menu, a data table's column menu). Idempotent.
 *
 * @param {ParentNode} root
 * @returns {() => void} detach
 */
export declare function attachScrollbars(root?: ParentNode): () => void;
/** The close label a consumer's markup can use: `data-kp-dialog-close` with the dictionary's word. */
export declare const closeLabel: () => string;
