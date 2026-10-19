/** Dispatched on the offending element, bubbling, with the Violation as detail. */
export declare const VIOLATION_EVENT = "kp-contract-violation";
export type Rule = "DI10" | "DI4";
export type Violation = {
  rule: Rule;
  element: Element;
  message: string;
};
/** @typedef {'DI10' | 'DI4'} Rule */
/** @typedef {{ rule: Rule, element: Element, message: string }} Violation */
/**
 * How long the `inline` variant stays armed.
 *
 * DI10's evidence, which is not the folklore: "undo beats confirmation"
 * has no controlled study behind it, while confirmations carrying a small
 * obstacle still worked for 44-74% of users after some twenty exposures,
 * against 20% or less for purely visual ones. Arm-then-act was that
 * obstacle until 4.0.0; it is now the `inline` variant, and the default
 * is the dialog TH107 asks for.
 *
 * Configurable rather than hard-coded, because it is an operational knob:
 * a dashboard whose users delete all day wants a longer window than a
 * settings page. Per element too, as `data-kp-confirm-ms`.
 */
export declare const CONFIRM_WINDOW_MS = 4000;
/** The two confirmation obstacles: the dialog TH107 asks for, and the arm-then-act 3.x shipped. */
export declare const CONFIRM_MODES: readonly ["dialog", "inline"];
/** Marks a button whose confirmation another channel already owns [AR29]. */
export declare const CONFIRM_OWNED = "[data-kp-confirm-owner]";
/** Fired on a typed confirmation block whenever what is typed starts or stops matching: `{ matched, value, name }`. */
export declare const TYPE_CONFIRM_EVENT = "kp-type-confirm";
/** A typed confirmation block a page puts in its own dialog or form. */
export declare const TYPE_CONFIRM = "[data-kp-type-confirm]";
/**
 * Whether `typed` is the name [kpadopt4-ideas]. Spaces at the start and the
 * end never count (a name pasted with a trailing space is the name); capitals
 * count unless `anyCase`. Nothing else is forgiven: the point is that the
 * reader looked at the name.
 * @param {string} typed @param {string} name @param {{ anyCase?: boolean }} [options]
 */
export declare function typeConfirmMatches(
  typed: string,
  name: string,
  {
    anyCase,
  }?: {
    anyCase?: boolean;
  },
): boolean;
/**
 * The typed confirmation [kpadopt4-ideas; Kenny, 2026-10-16]: what will be
 * lost, the field the name is typed into, the rule it is matched by and a
 * line that says whether it matches, in one block that keeps its height
 * while the reader types. `onChange(matched)` runs whenever the answer turns.
 * Used by openConfirmation() for `data-kp-confirm-type`, and by
 * attachTypeConfirms() for a block a page puts in its own dialog.
 *
 * @param {Document} doc
 * @param {{ name: string, anyCase?: boolean, loses?: Node | readonly string[] | null, strings?: Partial<import('./strings.js').Strings>, onChange?: (matched: boolean, value: string) => void, block?: HTMLElement }} options
 * @returns {{ block: HTMLElement, input: HTMLInputElement, matched: () => boolean }}
 */
export declare function buildTypeConfirm(
  doc: Document,
  {
    name,
    anyCase,
    loses,
    strings,
    onChange,
    block: given,
  }: {
    name: string;
    anyCase?: boolean;
    loses?: Node | readonly string[] | null;
    strings?: Partial<import("./strings.js").Strings>;
    onChange?: (matched: boolean, value: string) => void;
    block?: HTMLElement;
  },
): {
  block: HTMLElement;
  input: HTMLInputElement;
  matched: () => boolean;
};
/**
 * Open the modal confirmation for `button`, and call `onAccept` if it is
 * taken [TH107, T14, AR28].
 *
 * A native `<dialog>` shown with `showModal()`: the browser does the
 * focus trap, the Escape close and the focus return, which this package
 * already relies on twice. Escape and Cancel do nothing at all.
 *
 * **Written here rather than imported from `js/overlays.js`, on purpose.**
 * chassis-rs bakes six of this package's JavaScript modules into a Rust
 * binary, and the import closure of those six is exactly those six —
 * `js/overlays.js` is not among them. One import edge makes
 * `/static/kp/overlays.js` a 404, the module graph fail, and their whole
 * `chassis.js` die: the theme picker, the contract enforcer, the
 * confirmations and the skip links with it. `gates/check-closure.mjs`
 * holds that closure at six.
 *
 * Two opt-in checks for a removal that depends on something [kpadopt4]:
 * `dependents` lists what still uses the thing under the description (a
 * heading with the exact count, then one line each); `blocked` (the sentence
 * that says what to do first) makes it a notice instead of a question, with
 * one Close button and no Confirm. Each also comes from the button:
 * `data-kp-confirm-dependents` (one name per `;`) and
 * `data-kp-confirm-blocked` (empty: the dictionary's sentence).
 *
 * With `typeName` (or `data-kp-confirm-type` on the button) the reader
 * types that name before Confirm turns on [kpadopt4-ideas]: the dialog shows
 * what will be lost (`loses`, or the element `data-kp-confirm-loses` names),
 * the field, the rule (`anyCase`, `data-kp-confirm-case="any"`) and whether it
 * matches; Confirm stays disabled until it does, Enter in the field confirms a
 * match, and the field has the focus. Confirm carries `acceptLabel`
 * (`data-kp-confirm-label`), else the button's own words. A blocked removal
 * shows no field: there is nothing to confirm.
 *
 * @param {HTMLElement} button the control the confirmation belongs to
 * @param {{ phrase?: string, strings?: Partial<import('./strings.js').Strings>, onAccept?: () => void, onCancel?: () => void, className?: string, initialFocus?: 'cancel' | 'accept', dependents?: readonly string[], blocked?: string | boolean, typeName?: string, anyCase?: boolean, loses?: Node | readonly string[] | null, acceptLabel?: string }} [options]
 * @returns {HTMLDialogElement} the dialog, so a consumer can reach the state this set [KT6]
 */
export declare function openConfirmation(
  button: HTMLElement,
  {
    phrase,
    strings,
    onAccept,
    onCancel,
    className,
    initialFocus,
    dependents,
    blocked,
    typeName,
    anyCase,
    loses,
    acceptLabel,
  }?: {
    phrase?: string;
    strings?: Partial<import("./strings.js").Strings>;
    onAccept?: () => void;
    onCancel?: () => void;
    className?: string;
    initialFocus?: "cancel" | "accept";
    dependents?: readonly string[];
    blocked?: string | boolean;
    typeName?: string;
    anyCase?: boolean;
    loses?: Node | readonly string[] | null;
    acceptLabel?: string;
  },
): HTMLDialogElement;
/**
 * Wire every typed confirmation block a page put in its own dialog or form
 * [kpadopt4-ideas]:
 *
 *   <div data-kp-type-confirm="media" data-kp-type-confirm-for="wipe" data-kp-type-confirm-loses="wipe-list"></div>
 *   <button type="button" class="kp-button kp-button--destructive" id="wipe">Wipe media</button>
 *
 * The block is filled (what is lost, the field, the rule, the match line),
 * and the button `data-kp-type-confirm-for` names stays disabled until the
 * name is typed; `data-kp-type-confirm-case="any"` lets capitals pass. The
 * block fires `kp-type-confirm` when the answer turns. Detach empties the
 * block and gives the button back its own disabled state.
 *
 * @param {ParentNode} [root]
 * @param {{ strings?: Partial<import('./strings.js').Strings> }} [options]
 * @returns {() => void} detach
 */
export declare function attachTypeConfirms(
  root?: ParentNode,
  {
    strings,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
  },
): () => void;
/** Markup the consumer excludes from enforcement: `data-kp-contract-ignore`. */
export declare const EXEMPT = "[data-kp-contract-ignore]";
/**
 * @param {ParentNode} root
 * @param {{ rules?: Rule[], exempt?: string }} [options]
 * @returns {Violation[]}
 */
export declare function findViolations(
  root?: ParentNode,
  {
    rules,
    exempt,
  }?: {
    rules?: Rule[];
    exempt?: string;
  },
): Violation[];
/**
 * Report the violations and disarm what they point at.
 *
 * Idempotent: calling it again first restores everything it changed
 * before and then looks afresh, so markup completed after the first
 * pass comes back to life. Returns a detach that restores without
 * re-evaluating. The list is also available on the return value, so a
 * test asserts on it rather than on console output.
 *
 * @param {ParentNode} root
 * @param {{ disable?: boolean, rules?: Rule[], exempt?: string, log?: ((message: string, element: Element) => void) | null }} [options]
 * @returns {(() => void) & { violations: Violation[] }}
 */
export declare function enforceContracts(
  root?: ParentNode,
  {
    disable,
    rules,
    exempt,
    log,
  }?: {
    disable?: boolean;
    rules?: Rule[];
    exempt?: string;
    log?: ((message: string, element: Element) => void) | null;
  },
): (() => void) & {
  violations: Violation[];
};
/**
 * Give every destructive button that asked for a confirmation its
 * obstacle [TH107, DI10, AR27, AR29].
 *
 * Default `dialog`: the click is swallowed, a modal `<dialog>` opens
 * carrying the attribute's phrase, Escape and Cancel do nothing, and
 * Confirm re-fires the click behind the one-shot lock above, so the
 * consumer's own handler runs exactly once. `inline` is the arm-then-act
 * of 3.x, kept as a variant rather than deleted; `data-kp-confirm-mode`
 * chooses per element.
 *
 * @param {ParentNode} root
 * @param {{ mode?: 'dialog' | 'inline', windowMs?: number, disarmOnBlur?: boolean, strings?: Partial<import('./strings.js').Strings>, ownedBy?: string, dialogClassName?: string }} [options]
 * @returns {() => void} detach
 */
export declare function attachConfirmations(
  root?: ParentNode,
  {
    mode,
    windowMs,
    disarmOnBlur,
    strings,
    ownedBy,
    dialogClassName,
  }?: {
    mode?: "dialog" | "inline";
    windowMs?: number;
    disarmOnBlur?: boolean;
    strings?: Partial<import("./strings.js").Strings>;
    ownedBy?: string;
    dialogClassName?: string;
  },
): () => void;
/**
 * Move focus to the target of a skip link, adding `tabindex="-1"` if the
 * target cannot take focus on its own [KT6].
 *
 * JobTracker found the half a skip link needs and nothing here provided:
 * without a focusable target the browser scrolls and the next Tab goes
 * back into the menu, so the link has done nothing for the person it
 * exists for. Returns whether a target was found and focused.
 *
 * @param {string} href `#main`, or any same-page hash
 * @param {Document | Element} [root]
 * @returns {boolean}
 */
export declare function skipTo(
  href: string,
  root?: Document | Element,
): boolean;
/** Fired on the control when it appears or goes away: `{ shown }`. */
export declare const TO_TOP_EVENT = "kp-to-top";
/** The attribute that marks a back-to-top control [feat-page-1]. */
export declare const TO_TOP = "[data-kp-to-top]";
/**
 * Wire every back-to-top control under `root` [feat-page-1].
 *
 * Two halves, and the second is the one usually missing: it takes the
 * reader back, and it takes the FOCUS back. A control that only scrolls
 * leaves a keyboard user at the bottom of the document with the view at
 * the top, which is worse than not moving at all.
 *
 * Where the focus lands is `data-kp-to-top-target`, a selector, and it
 * falls back to the document's own body — the top of the page, which is
 * what the control is named after. It is NOT the skip link's target: on a
 * page with anything tall above the content, focusing that landmark
 * scrolls straight back down to it and undoes the journey. Scrolling is
 * left to the browser: `scrollTo` follows the root's own scroll-behaviour,
 * which is the knob feat-layout-2 already put there and which a reader who
 * asked for less motion has already overruled.
 *
 * @param {ParentNode} root
 * @param {{ strings?: Partial<import('./strings.js').Strings>, after?: number }} [options]
 * @returns {() => void} detach
 */
export declare function attachToTop(
  root?: ParentNode,
  {
    strings,
    after,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
    after?: number;
  },
): () => void;
/** Fired on the nav when its toggle opens or closes it: `{ open }`. */
export declare const NAV_TOGGLE_EVENT = "kp-nav-toggle";
/** The mark the React NavBar puts on a toggle it wires itself [AR29]. */
export declare const NAV_OWNED = "[data-kp-nav-owner]";
/**
 * Wire the toggle a narrow navigation collapses into [scope-10, stage 1.3].
 *
 * Opt-in by the button being there: a nav without one keeps the behaviour
 * it had, which is what makes this additive for every page already built.
 * The CSS decides when the bar is narrow enough to collapse; this decides
 * nothing about width at all, so the two cannot disagree.
 *
 * Every state it sets has a way out [KT6]: the toggle itself, Escape while
 * the focus is inside the nav, a click outside it, and the `kp-nav-toggle`
 * event for a consumer who wants to persist or veto nothing but observe.
 * `detach` removes what attach stamped.
 *
 * @param {ParentNode} root
 * @param {{ strings?: Partial<import('./strings.js').Strings>, ownedBy?: string }} [options]
 * @returns {() => void} detach
 */
export declare function attachNavToggles(
  root?: ParentNode,
  {
    strings,
    ownedBy,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
    ownedBy?: string;
  },
): () => void;
/** Fired on the wrapper when a sticky bar turns compact or back: `{ compact }`. */
export declare const NAV_COMPACT_EVENT = "kp-nav-compact";
/** The mark the React NavBar puts on a sticky wrapper it wires itself [AR29]. */
export declare const NAV_STICKY_OWNED = "[data-kp-nav-sticky-owner]";
/**
 * Wire every sticky nav bar under `root` [scope-48 wave 2].
 *
 * Opt-in by the modifier `.kp-nav-wrap--sticky`; the CSS makes the bar
 * stick, this decides when it is compact and tells the scrolling box how
 * tall it is. Two things, both undone by `detach`:
 *
 * - `data-kp-nav-compact` on the wrapper once the box has scrolled further
 *   than `data-kp-nav-sticky-after` (px, and never less than the bar's own
 *   height at rest, which is the default), and off again once it is back
 *   within that distance less the bar's height. The gap between the two is not decoration: shrinking the
 *   bar moves the content under it up, the browser's scroll anchoring moves
 *   the scroll position with it, and without a gap at least that wide the
 *   bar would flip between its two heights at the threshold.
 * - `--kp-nav-sticky-height` and `data-kp-nav-sticky-root` on the scrolling
 *   box (the document's root, or the nearest ancestor that scrolls), which
 *   the box's `scroll-padding-block-start` reads, so an anchor or a focused
 *   element lands below the bar. A page's own `--kp-scroll-offset` wins.
 *
 * @param {ParentNode} root
 * @param {{ ownedBy?: string, after?: number }} [options]
 * @returns {() => void} detach
 */
export declare function attachStickyNavs(
  root?: ParentNode,
  {
    ownedBy,
    after,
  }?: {
    ownedBy?: string;
    after?: number;
  },
): () => void;
/**
 * One sticky bar; the React NavBar calls this for the wrapper it renders.
 *
 * @param {HTMLElement} wrap the `.kp-nav-wrap--sticky` element
 * @param {number} [after] px; overrides `data-kp-nav-sticky-after`
 * @returns {() => void} detach
 */
export declare function stickyNav(
  wrap: HTMLElement,
  after?: number,
): () => void;
/**
 * Hang a bar item's open dropdown from whichever edge keeps it in the
 * window [fix-27].
 *
 * The panel hangs from its item's start edge. Under the last item of a bar
 * whose links sit at the window's end, that ran it past the window's right
 * edge in all 22 themes (grotesk's catalogue bar, Kenny's note of
 * 2026-09-14). A stylesheet cannot see where the window ends, so this
 * measures the open panel on both edges and keeps the one that lies less
 * outside — the start edge when both fit — writing
 * `data-kp-nav-menu-end` on the panel for the other. Both channels call it:
 * the module on hover and focus, the React NavBar from its item.
 *
 * Changing edges is not always enough, because the box that shows the bar
 * is not always the window [fix-39]: in a box that clips, both of an item's
 * edges can lie inside the window while the panel under it is cut. So the
 * edge is chosen against the box the panel is really seen in, and what
 * still hangs out afterwards is slid back in (`slideIntoView`).
 *
 * @param {Element} item the `.kp-nav__links > li` that holds the dropdown
 * @param {boolean} [retry] measure once more on the next frame when the panel is not open yet; default true
 * @returns {boolean} whether the panel now hangs from the end edge
 */
export declare function placeNavMenu(item: Element, retry?: boolean): boolean;
/**
 * Line an open mega menu's panel up with its bar's edges [scope-48].
 *
 * The panel is placed in whatever box contains it, and the registers
 * disagree about which box that is: the `.kp-nav-wrap` in the base, the
 * bar itself in eight registers, and cyberpunk's strip of links, which
 * must stay positioned because its cut-corner plate hangs from it. So the
 * bar is measured against that box, and the two offsets are written as
 * `--kp-nav-mega-start` and `--kp-nav-mega-end` on the panel.
 *
 * Lined up with its bar the panel is normally inside whatever shows the
 * bar, but it is placed against a box and judged against another, so it is
 * held to the same question the dropdown is [fix-39]: a bar wider than the
 * box that shows it would otherwise carry its panel out with it.
 *
 * @param {Element} panel the `.kp-nav__menu--wide`, open
 */
export declare function placeNavPanel(panel: Element): void;
/**
 * Wire every bar's dropdowns and mega menus [fix-27, scope-48].
 *
 * Dropdowns open on hover and on focus within, in CSS; this only places an
 * open one so it stays in the window (`placeNavMenu`), when it opens and
 * again when the window changes size.
 *
 * A mega menu is a `data-kp-nav-disclosure` button beside a
 * `.kp-nav__menu--wide` panel. The button gets `aria-expanded` and an
 * `aria-controls` naming the panel (an id is given when the panel has
 * none), and the panel shows while it says `true`. Every open state has a
 * way out [KT6]: the button again, Escape while the focus is in the bar
 * (the focus goes back to the button), a click outside the item, and the
 * focus leaving it. One panel is open at a time: opening one closes the
 * others. A button with no words of its own is named from the dictionary
 * (`navDisclosure`). The React NavBar wires its own bar and marks it
 * `data-kp-nav-owner`, so this module leaves that bar alone [AR29].
 *
 * @param {ParentNode} root
 * @param {{ strings?: Partial<import('./strings.js').Strings>, ownedBy?: string }} [options]
 * @returns {() => void} detach
 */
export declare function attachNavMenus(
  root?: ParentNode,
  {
    strings,
    ownedBy,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
    ownedBy?: string;
  },
): () => void;
/**
 * Make every `.kp-skip-link` (or `[data-kp-skip]`) move focus, not only
 * the scroll position.
 *
 * @param {ParentNode} root
 * @returns {() => void} detach
 */
export declare function attachSkipLinks(root?: ParentNode): () => void;
/**
 * Show `word` in a `.kp-state-word`, adding it to the words it keeps room
 * for when it is new, so the width never shrinks back under a later word
 * [scope-143]. The element is as wide as the widest word listed in its
 * `data-kp-words` (one per line), so a button beside it never moves when
 * the state changes: a second click without moving the mouse hits the same
 * button. The stylesheet does the holding; this only keeps the list whole.
 *
 * @param {HTMLElement} el
 * @param {string} word
 */
export declare function setStateWord(el: HTMLElement, word: string): void;
