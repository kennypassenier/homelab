// The kp-themes module chassis-rs serves at /static/kp/js/sidenav.js (kp-themes 11.0.0),
// copied from kp-themes js/sidenav.d.ts: chassis serves the module, not its types.
export declare const SIDENAV_TOGGLE_EVENT = "kp-sidenav-toggle";
export declare const SIDENAV_SLIM_EVENT = "kp-sidenav-slim";
export declare const SIDENAV_MODE_EVENT = "kp-sidenav-mode";
/** A change that was asked for and refused: by a listener, or by a static backdrop [Bootstrap's hidePrevented]. */
export declare const SIDENAV_PREVENTED_EVENT = "kp-sidenav-prevented";
/** The mark a consumer puts on a toggler they wire themselves [AR29]. */
export declare const SIDENAV_OWNED = "[data-kp-sidenav-owner]";
/**
 * Every option, written out.
 *
 * Built by joining a prefix to a name at first, which worked and hid the
 * whole surface: no gate could enumerate the options, and the
 * documentation site's extractor found one attribute whose name was the
 * bare prefix. An option nobody can list is an option nobody can
 * document [AR21].
 */
export declare const OPTIONS: {
  mode: string;
  position: string;
  side: string;
  open: string;
  slim: string;
  slimCollapsed: string;
  expandOnHover: string;
  accordion: string;
  backdrop: string;
  backdropClass: string;
  closeOnEsc: string;
  lockScroll: string;
  focusTrap: string;
  content: string;
  /** The older spelling of `data-kp-remember`, still read [js/remember.js]. */
  remember: string;
  toggle: string;
  slimToggle: string;
  slimHide: string;
  slimShow: string;
  expanded: string;
  overBelow: string;
  narrow: string;
  /**
   * On the panel: the content named by `data-kp-sidenav-content` keeps the
   * panel's width whether it is open or not (push and over modes) — the
   * gutter the panel opens into. Default off.
   */
  reserve: string;
  /**
   * On the panel: which opener the page shows, `launcher` (the fixed
   * `.kp-sidenav__launcher`), `tab` (the `.kp-sidenav__tab` on the edge) or
   * `both`. Absent (or any other value): `tab`, the default [Kenny, 2026-10-14];
   * a page with no `.kp-sidenav__tab` for the panel keeps its launcher.
   * Applies to `over` and `push`; `side` and the collapsed rail ignore it.
   */
  opener: string;
  /** Written on an opener box (launcher or tab) the choice turns off; the box is not drawn. */
  openerOff: string;
  /** Written on an opener box the choice keeps, once the module has decided (the first paint hides the others by the markup alone). */
  openerOn: string;
  /** Written on the panel while a `.kp-sidenav__launcher` drives it. */
  launched: string;
  /** Written on the launcher box from the opening's first frame to the closing's last. */
  lit: string;
  /** On a page element in the launcher's corner: keeps the launcher's width free at its start. */
  launcherRoom: string;
  /**
   * On the panel: a closed panel is `inert` (out of the tab order and the
   * accessibility tree) from the closing's last frame to the opening's
   * first. Default on; `"false"` leaves a closed panel reachable.
   */
  inert: string;
  /**
   * On the panel: in the collapsed rail each item shows its label as the
   * package's tooltip on hover and on focus. Default on; `"false"` turns it off.
   */
  railTips: string;
  /**
   * On a panel with `data-kp-sidenav-slim`: the width of the panel's box at
   * and below which the rail collapses to its icons, a length, or the
   * package's 40rem when empty; wider again it is what the reader left it.
   * `data-kp-sidenav-over-below` is its sibling for the variant: below its
   * length the panel becomes an `over` panel.
   */
  slimBelow: string;
  /**
   * On the panel: in the collapsed rail a group's submenu opens as a
   * floating panel beside the rail, on hover (with intent) and from the
   * keyboard, with the theme's popover effect. Default on; `"false"` keeps
   * groups folding in place.
   */
  flyout: string;
  /** Written on a `.kp-sidenav__category` while its submenu floats beside the rail. */
  flown: string;
  /**
   * On the panel: a key chord that toggles it from anywhere but a text
   * field, e.g. `Control+b`. `Control` means Meta (⌘) on a Mac; `Mod` is
   * the same thing spelled for both. Off by default.
   */
  shortcut: string;
  /**
   * On the panel: a selector, inside the panel, for where the focus lands
   * when the panel opens. Default: the first focusable element, and only
   * for an `over` panel; named, it is used in every mode.
   */
  initialFocus: string;
  /**
   * On a rail with `data-kp-sidenav-expand-on-hover`: how long the pointer
   * rests before the rail expands, in milliseconds. Default 150. The rail
   * also expands while the focus is inside it.
   */
  hoverDelay: string;
  /** Written on the rail while it is expanded by the pointer or the focus. */
  peek: string;
  /**
   * On the panel: `kp-sidenav-toggle` and `kp-sidenav-slim` fire BEFORE the
   * change, cancelable, with `detail.reason`; `preventDefault()` leaves the
   * state untouched and `kp-sidenav-prevented` fires. Off by default: the
   * events then fire after the change, as they always did, now with
   * `detail.reason` as well.
   */
  cancelable: string;
  /**
   * On the panel: `"false"` makes a click on the backdrop do nothing, the
   * sibling of `data-kp-sidenav-close-on-esc`. Only a panel that has a
   * backdrop is concerned. Default on.
   */
  closeOnBackdrop: string;
  /** Written on the panel and its backdrop while the refused cue plays (a static backdrop was clicked). */
  refused: string;
  /**
   * On the panel: a grab handle on the edge that faces the content makes the
   * width the reader's to change; it writes `--kp-sidenav-width` and, with
   * `remember`, keeps it. Off by default; `"false"` is off.
   */
  resizable: string;
  /** On a resizable panel: the narrowest width the handle allows, a length. Default 12rem. */
  minWidth: string;
  /** On a resizable panel: the widest width the handle allows, a length. Default 32rem. */
  maxWidth: string;
  /** Written on the panel while the handle is held. */
  resizing: string;
  /**
   * On the panel: its look. Without it the panel is the flush column it
   * always was. `"floating"` detaches the panel from the window's edge: a
   * gap all round, the theme's radius, an edge on every side and a shadow
   * made of the theme's own tokens. Knobs: `--kp-sidenav-float-inset` (the
   * gap, `--kp-space-sm`), `--kp-sidenav-float-radius` (`--radius`),
   * `--kp-sidenav-float-edge` (`1px solid --border-strong`),
   * `--kp-sidenav-float-shadow`. `"inset"` keeps the panel flush with the
   * window and makes the content (the element `data-kp-sidenav-content`
   * names) a rounded card beside it, on the page's ground. Knobs:
   * `--kp-sidenav-inset-gap` (the card's margin, `--kp-space-sm`),
   * `--kp-sidenav-inset-radius` (`--radius`), `--kp-sidenav-inset-edge`
   * (`1px solid --border-strong`). Both keep the slide, the push, the
   * gutter and the over mode working: the push and the gutter leave the
   * room the variant takes, and the edge tab rides on the panel's edge.
   */
  variant: string;
  /**
   * Written on the content element while the panel is the `inset` variant:
   * the edge the panel comes from, `start` or `end`. The content is a
   * rounded card while it is there.
   */
  inset: string;
  /**
   * On the panel: `"compact"` or `"comfortable"` sets the tokens that make
   * a row's height and the panel's padding (`--kp-sidenav-link-padding-block`,
   * `-link-size`, `-icon-size`, `-pad`); without it, or with `"default"`,
   * the panel is what it always was. CSS only; the module does not read it.
   */
  density: string;
};
/** The class of the handle the module adds to a resizable panel. */
export declare const SIDENAV_RESIZER = ".kp-sidenav__resizer";
/** The fixed box at the panel's far edge that holds a small toggler: the edge tab. */
export declare const SIDENAV_TAB = ".kp-sidenav__tab";
/** The fixed box at the window's start corner that holds a panel's toggler. */
export declare const SIDENAV_LAUNCHER = ".kp-sidenav__launcher";
/**
 * A key chord as written on the panel: `Control+b`, `Mod+Shift+k`, `Alt+n`.
 * `Control` and `Mod` are the platform's command key: Meta on a Mac.
 *
 * @param {string} chord
 * @param {boolean} mac
 */
export declare function parseChord(
  chord: string,
  mac: boolean,
): {
  key: string;
  ctrl: boolean;
  meta: boolean;
  alt: boolean;
  shift: boolean;
  /** The same chord as `aria-keyshortcuts` writes it. */
  aria: string;
};
export type Sidenav = {
  element: HTMLElement;
  open: (options?: { remember?: boolean }) => void;
  /**
   * `{ remember: false }` for a close the reader did not ask for — a layout adapting to the room it has
   */
  close: (options?: { remember?: boolean }) => void;
  toggle: (options?: { remember?: boolean }) => void;
  setMode: (mode: "over" | "side" | "push") => void;
  setSlim: (collapsed?: boolean) => void;
  isOpen: () => boolean;
  destroy: () => void;
};
/**
 * @typedef {object} Sidenav
 * @property {HTMLElement} element
 * @property {(options?: { remember?: boolean }) => void} open
 * @property {(options?: { remember?: boolean }) => void} close  `{ remember: false }` for a close the reader did not ask for — a layout adapting to the room it has
 * @property {(options?: { remember?: boolean }) => void} toggle
 * @property {(mode: 'over' | 'side' | 'push') => void} setMode
 * @property {(collapsed?: boolean) => void} setSlim
 * @property {() => boolean} isOpen
 * @property {() => void} destroy
 */
/**
 * The handle for one panel, or undefined when it was never attached.
 *
 * @param {Element | null} element
 * @returns {Sidenav | undefined}
 */
export declare function sidenavOf(element: Element | null): Sidenav | undefined;
/**
 * Wire every `.kp-sidenav` under `root`.
 *
 * @param {ParentNode} root
 * @param {{ strings?: Partial<import('./strings.js').Strings>, ownedBy?: string, store?: Storage | null }} [options]
 * @returns {() => void} detach
 */
export declare function attachSidenavs(
  root?: ParentNode,
  {
    strings,
    ownedBy,
    store,
  }?: {
    strings?: Partial<import("./strings.js").Strings>;
    ownedBy?: string;
    store?: Storage | null;
  },
): () => void;
