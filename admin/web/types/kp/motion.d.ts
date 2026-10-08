// The kp-themes module chassis-rs serves at /static/kp/js/motion.js (kp-themes 10.0.0),
// copied from kp-themes js/motion.d.ts: chassis serves the module, not its types.
/** Any box a consumer wants eased, beside the components below. */
export declare const SIZE_ATTRIBUTE = "data-kp-size-motion";
/** The boxes that ease to a new size. */
export declare const SIZE_SELECTOR: string;
/** The disclosures that unfold and fold back. */
export declare const FOLD_SELECTOR = ".kp-accordion__item";
/**
 * The attributes that give a row its stable id, in the order they are read,
 * for `data-kp-arrive="new"` [port spec G]: a row added back under the key
 * of a row that just left is the same row, repainted.
 */
export declare const ARRIVE_KEYS: readonly string[];
/**
 * How many dialogs, boxes and disclosures motion is watching right now. A
 * diagnostic: after a page removes its boxes it returns to where it was,
 * because a box that leaves the page is let go.
 * @returns {number}
 */
export declare function motionWatchCount(): number;
/**
 * The theme's motion, read from its dialog entrance: a dialog drawn for an
 * instant out of sight, in the theme `scope` wears.
 *
 * @param {Element} [scope] where the theme is read (its closest `[data-theme]` applies)
 * @returns {{ open: number, ease: string, close: number, size: number }} milliseconds, and the entrance's curve
 */
export declare function themeMotion(scope?: Element): {
  open: number;
  ease: string;
  close: number;
  size: number;
};
/**
 * Close a dialog the way it opened, backwards, then close it for real. With
 * no entrance to reverse (reduced motion, a theme without one) it closes at
 * once.
 *
 * @param {HTMLDialogElement} dialog
 * @param {string} [returnValue]
 * @returns {Promise<void>} settled once the dialog is closed
 */
export declare function closeDialog(
  dialog: HTMLDialogElement,
  returnValue?: string,
): Promise<void>;
/** The attribute playEntranceBackwards() sets while it turns an entrance around. */
export declare const REVERSING_ATTRIBUTE = "data-kp-reversing";
/**
 * Play the CSS entrance `el` wears now backwards, then settle: a tooltip
 * that slid in slides out, a tour card that faded in fades out, each in its
 * entrance's own time on its curve turned around, so frame t of the going
 * is frame (T - t) of the coming (Kenny, 2026-10-05: "find out where else
 * there is a discrepancy between opening/closing"). The entrance is
 * restarted reversed on the element and its pseudo-elements
 * (`[data-kp-reversing]`, css/components.css), whatever its fill, so one
 * that already played out is turned around too. With no entrance (reduced
 * motion, a theme without one) it settles at once. The caller hides the
 * element when it settles true and then calls stopReversing(), which also
 * stops it early when the element is wanted back meanwhile.
 * @param {HTMLElement} el
 * @returns {Promise<boolean>} true once played out, false when there was nothing to play or it was stopped
 */
export declare function playEntranceBackwards(
  el: HTMLElement,
): Promise<boolean>;
/**
 * Stop a playEntranceBackwards() on `el` (it is wanted back), or tidy up
 * after one that played out: a running one settles false, and the entrance
 * is the element's own again.
 * @param {HTMLElement} el
 */
export declare function stopReversing(el: HTMLElement): void;
/** The attribute a panel carries while playClose() plays its close. */
export declare const CLOSING_ATTRIBUTE = "data-kp-closing";
/** The panels that hang from a trigger and close as their opening reversed. */
export declare const PANEL_SELECTOR =
  ".kp-popover:not(.kp-tooltip), .kp-menu, .kp-combobox__list, .kp-datepicker__panel";
/**
 * Play the close of a panel that is about to be hidden (a menu, a popover):
 * its opening played backwards, frame for frame, in the opening's own time
 * (Kenny, 2026-10-06: every close is its open played backwards; menus and
 * popovers hid at once until now, research/PACKAGE_FINDINGS.md). The panel
 * carries `[data-kp-closing]` meanwhile, so a register can draw a close of
 * its own there instead (blueprint: the same trace under a second name, so
 * the browser restarts it) or run its transitions to the closed state
 * (synthwave). The caller hides the panel once this settles true, then calls
 * stopClose(); a stopClose() before that (the panel wanted back) settles it
 * false and leaves the opening where it stands.
 * @param {HTMLElement} el
 * @returns {Promise<boolean>} true once the close played out (or there was nothing to play)
 */
export declare function playClose(el: HTMLElement): Promise<boolean>;
/**
 * Tidy up after playClose() (call it once the panel is hidden), or stop one
 * still playing because the panel is wanted back: it turns round from the
 * frame on screen.
 * @param {HTMLElement} el
 */
export declare function stopClose(el: HTMLElement): void;
/**
 * The entrance's curve without its overshoot: a size goes to its new value
 * and stops there (Kenny, 2026-10-04: pastel's and synthwave's cards "grow
 * too much and shrink again at the end, it should just grow to the correct
 * size"). A cubic-bezier's y values are held between 0 and 1.
 * @param {string} ease
 */
export declare function withoutOvershoot(ease: string): string;
/**
 * A timing function turned around: the curve that plays a motion backwards
 * as `direction: reverse` would, so a close, a shrink or a fold that runs
 * on it is its opening frame for frame in the other order. A cubic-bezier
 * is rotated half a turn (an ease-out becomes an ease-in, an overshoot past
 * the end becomes one before the start), steps that jump at the end jump at
 * the start, a `linear()` list runs from its other end; `linear` and the
 * symmetric curves stay themselves.
 * @param {string} ease a CSS easing function
 * @returns {string}
 */
export declare function reversedEase(ease: string): string;
/**
 * The size motion of the theme `box` wears, for a box this module does not
 * glide itself (the tiles of a set, js/tiles.js): the duration and the curve
 * a change of `change` px runs on, with the theme's own size character
 * (`--kp-size-ease`, `--kp-size-steps`). A duration of 0 means: take the
 * new size at once (reduced motion, a theme without an entrance, or a theme
 * switch in progress).
 * @param {HTMLElement} box @param {number} change in px
 * @returns {{ duration: number, easing: string }}
 */
export declare function sizeMotion(
  box: HTMLElement,
  change: number,
): {
  duration: number;
  easing: string;
};
/**
 * Play backwards the arrival arrive() gives `el` in a theme where it does
 * not arrive as a leave turned around (a table row; Kenny, 2026-10-05: the
 * rows of a group unfolded the theme's way and folded away at once): the
 * register's own `[data-kp-arriving]`, else the theme's toast entrance, in
 * its own time on its curve turned around. The caller hides `el` once it
 * settles. Settles at once when there is no arrival to turn around.
 * @param {HTMLElement} el
 * @returns {Promise<void>}
 */
export declare function playArrivalBackwards(el: HTMLElement): Promise<void>;
export type Arriving = {
  el: HTMLElement;
  plays: Animation[];
  total: number;
  lag: number;
};
/**
 * Let `el` leave the theme's way, then take it out [scope-142; Kenny,
 * 2026-10-04: "die grow/shrink bewegingen moeten ook zijn als er opeens
 * nieuwe elementen bijkomen of weggaan"]: it plays its arrival backwards
 * while it folds shut, so what is under it closes up instead of jumping,
 * and the box around it shrinks with it. Under reduced motion, or in a theme
 * with no arrival, it goes at once.
 *
 * Elements told to leave in the same task leave one by one, bottom first.
 *
 * @param {HTMLElement} el
 * @param {{ hide?: boolean }} [options] `hide: true` sets `hidden` instead of removing it
 * @returns {Promise<void>} settled once it is gone
 */
export declare function leave(
  el: HTMLElement,
  {
    hide,
  }?: {
    hide?: boolean;
  },
): Promise<void>;
export type Leaving = {
  el: HTMLElement;
  hide: boolean;
  resolve: () => void;
};
export type RepaintNode = {
  getAttribute: (name: string) => string | null;
  tagName: string;
  className: string;
  id?: string;
  nodeType: number;
  matches?: (selector: string) => boolean;
  querySelector?: (selector: string) => unknown;
};
/**
 * @typedef {{ getAttribute: (name: string) => string | null, tagName: string, className: string, id?: string, nodeType: number, matches?: (selector: string) => boolean, querySelector?: (selector: string) => unknown }} RepaintNode
 */
/**
 * The added elements of one batch of changes that only repaint what was
 * there [port spec G; the homelab dashboard's `repaints()`, moved here]:
 * an element is a repaint when its parent lost, in the same batch, an
 * element with its key (the first of `keys` it carries) or, with no key,
 * one more element of its tag and class than it already took back. An
 * element added where a loading skeleton left (`.kp-skeleton`,
 * `[data-kp-skeleton]`) is not news either: it is the data the skeleton
 * waited for. Everything else is new content.
 *
 * Measured by the dashboard (8 quiet seconds per page): without this, a
 * live refresh that redrew its rows replayed the arrival on 158 cells of
 * one page, 112 on another and 72 on a third, though nothing new had come.
 *
 * @param {Iterable<{ type: string, target: unknown, addedNodes: ArrayLike<any>, removedNodes: ArrayLike<any> }>} records
 * @param {{ keys?: readonly string[] }} [options]
 * @returns {Set<any>} the added elements that are repaints
 */
export declare function repaintedIn(
  records: Iterable<{
    type: string;
    target: unknown;
    addedNodes: ArrayLike<any>;
    removedNodes: ArrayLike<any>;
  }>,
  {
    keys,
  }?: {
    keys?: readonly string[];
  },
): Set<any>;
/** The value of `data-kp-folding` on what folds out of a box: it is gone from the box's layout once its fold has played. */
export declare const FOLDING_OUT = "out";
export type SizeOptions = {
  /**
   * which added elements arrive the theme's way when no `data-kp-arrive` says otherwise: every one (`all`, the default), only those that are not a repaint of a row that just left under the same key (`new`), or none
   */
  arrive?: "all" | "new" | "none";
  /**
   * more attributes that carry a row's stable id, read before `data-kp-key`, `data-kp-row-key` and `id`
   */
  arriveKeys?: readonly string[];
};
/**
 * @typedef {object} SizeOptions
 * @property {'all' | 'new' | 'none'} [arrive] which added elements arrive the theme's way when no `data-kp-arrive` says otherwise: every one (`all`, the default), only those that are not a repaint of a row that just left under the same key (`new`), or none
 * @property {readonly string[]} [arriveKeys] more attributes that carry a row's stable id, read before `data-kp-key`, `data-kp-row-key` and `id`
 */
/**
 * Ease `box` to its new height whenever what is in it changes size, in both
 * directions; a change during a glide continues from where the box is.
 * @param {HTMLElement} box
 * @param {SizeOptions} [options]
 * @returns {() => void}
 */
export declare function easeSize(
  box: HTMLElement,
  { arrive: fallback, arriveKeys }?: SizeOptions,
): () => void;
/**
 * Give every dialog its leaving motion and every box above its easing, under
 * `root` and in whatever is added to it later. A box, dialog or disclosure
 * that leaves the page is let go (its observers disconnected) a microtask
 * after it left, so a page that rebuilds itself on every navigation does not
 * keep the old boxes' watchers alive; one moved within the page stays.
 *
 * @param {ParentNode} [root]
 * @param {SizeOptions & { size?: string }} [options] `size`: more boxes to ease, as a selector, beside the package's own and `[data-kp-size-motion]` (a consumer's cards and panels, without marking each one); `arrive` and `arriveKeys` as for easeSize()
 * @returns {() => void} detach
 */
export declare function attachMotion(
  root?: ParentNode,
  {
    size,
    arrive,
    arriveKeys,
  }?: SizeOptions & {
    size?: string;
  },
): () => void;
