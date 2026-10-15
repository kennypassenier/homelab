// The kp-themes module chassis-rs serves at /static/kp/js/effects.js (kp-themes 10.1.1),
// copied from kp-themes js/effects.d.ts: chassis serves the module, not its types.
export declare const HOOKS: Readonly<{
  surface: "data-kp-surface";
  reveal: "data-kp-reveal";
  revealTrigger: "data-kp-reveal-trigger";
  revealEvery: "data-kp-reveal-every";
  divider: "data-kp-divider";
  /** A row of items a theme may run [M1, 2026-09-08]. */
  marquee: "data-kp-marquee";
  label: "data-kp-label";
  /** The label a stamp takes once the file is open [S49, A11]. */
  labelOpen: "data-kp-label-open";
  /** Set on the container while the file is open. */
  openState: "data-kp-open";
  navSide: "data-kp-nav-side";
  /**
   * A number that counts up to what it already says [feat-count-1].
   *
   * The element's authored text is the truth and the module never
   * invents one: it reads the number out of that text, counts to it,
   * and puts the text back exactly as written. A page that never
   * attaches this module, or a reader who asked for less movement,
   * sees the final number and nothing else — which is the frozen bar.
   */
  count: "data-kp-count";
  /** `armed` | `running` | `done`, readable at any moment [KT16]. */
  countState: "data-kp-count-state";
}>;
/** The surfaces a section can stand on [TH116]. */
export declare const SURFACES: readonly string[];
/** What an element can be revealed as [TH119, TH120, TH122]. */
export declare const REVEALS: readonly string[];
/**
 * The routine names a theme may put in a reveal knob, and the arrival
 * names it may put in `--kp-arrival` [G6, 2026-09-08]. A value outside
 * these lists used to fall through the whole chain to the last branch —
 * the glyph noise of cyberpunk — so one letter wrong in a register gave a
 * theme the loudest effect in the package with no warning at all. It is
 * reported as `kp-effect-unknown` now, exactly like an unknown surface,
 * and the element rests instead.
 */
export declare const HEADLINE_ROUTINES: readonly string[];
/** What a theme may ask of the page's arrival: synthwave's boot line, phantom's calling card. */
export declare const ARRIVALS: readonly string[];
/**
 * The state classes this module toggles, and nothing else. Contract
 * values: a consumer may select on them, a register does.
 */
export declare const STATE: Readonly<{
  registering: "is-registering";
  revealing: "is-revealing";
  sharpening: "is-sharpening";
  in: "is-in";
  cleared: "is-cleared";
  deciphered: "is-deciphered";
  glitching: "is-glitching";
  noise: "is-noise";
  tracking: "is-tracking";
  shine: "is-shine";
  off: "is-off";
  words: "is-words";
  dissolving: "is-dissolving";
  typing: "is-typing";
  popping: "is-popping";
  settling: "is-settling";
  calibrating: "is-calibrating";
  revealed: "is-revealed";
  gilding: "is-gilding";
}>;
/**
 * The custom properties a theme declares to say which reveals it performs
 * [S45]: `--kp-reveal-headline: decipher`, `--kp-reveal-emphasis:
 * classified`, `--kp-reveal-rule: draw`. Absent or empty means quiet.
 */
export declare const ROUTINES: Readonly<{
  headline: "--kp-reveal-headline";
  emphasis: "--kp-reveal-emphasis";
  rule: "--kp-reveal-rule";
  arrival: "--kp-arrival";
}>;
/** The class names of the arrival overlay the module builds. */
export declare const ARRIVAL: Readonly<{
  root: "kp-boot";
  line: "kp-boot__line";
  skip: "kp-boot__skip";
  bar: "kp-boot__bar";
}>;
/** The knob a theme sets to put a block cursor inside its fields [TM2, R6-Q7]: `--kp-caret: block`. */
export declare const CARET_KNOB = "--kp-caret";
/** The knobs a theme's own boot reads [S49, A11]. */
export declare const KNOBS: Readonly<{
  /** `block` builds the segmented bar retro's POST counts along. */
  arrivalBar: "--kp-arrival-bar";
  /** What a `{count}` in a boot line counts up to. Default 640, as a memory test reads. */
  arrivalCount: "--kp-arrival-count";
  /**
   * Whether a click anywhere on the arrival overlay ends it [CP1].
   *
   * `anywhere` (the default since 6.0.0) or `skip-only` for what it did
   * before. The overlay is `position: fixed; inset: 0`, so until now it
   * ate every click for up to 1100ms and only the Skip button ended it —
   * a click elsewhere did nothing and gave no sign it had been lost.
   * JobTracker reported that as "the theme picker does not work on
   * phantom"; the picker was fine.
   */
  arrivalDismiss: "--kp-arrival-dismiss";
  /**
   * How fast the arrival plays, as a factor [scope-84]. Default 1.
   *
   * Every wait of the arrival — a boot line's step, a percentage's step,
   * the card's hold, the pause before it switches off — is divided by
   * it, and every CSS animation on the overlay (the CRT switching off,
   * the card's bar and its shove) plays at it as its playback rate. So
   * `0.5` takes twice as long and `2` half as long, and the sequence
   * stays the same sequence. A value that is not a number above zero
   * reads as 1. The catalogue's intro inspector (catalogue/intros.html)
   * sets it on the root of a frame; no register declares it, and a page
   * that never sets it plays exactly as before.
   */
  arrivalRate: "--kp-arrival-rate";
}>;
/**
 * How long a counting number takes, in milliseconds [feat-count-1].
 * A theme sets `--kp-count: 1200`; the default is 900. `0` — or the
 * reduced-motion setting, which always wins — puts the number there at
 * once without ever having counted.
 */
export declare const COUNT_KNOB = "--kp-count";
/** Where a counting number starts. Default 0; a theme or a page may set another. */
export declare const COUNT_FROM_KNOB = "--kp-count-from";
/** The custom property the arrival bar's fill reads, 0 to 1. */
export declare const BOOT_PROGRESS = "--kp-boot-progress";
/** How long one full pass of a marquee takes [M1, 2026-09-08]. */
export declare const MARQUEE_KNOB = "--kp-marquee";
/** Whether a marquee rests while it is off screen: `offscreen` (default) or `never` [M2]. */
export declare const MARQUEE_PAUSE_KNOB = "--kp-marquee-pause";
/** The knob blueprint sets to run its own live dimension lines [S48, LIFT_PLAN row 6]: `--kp-measure: live`. */
export declare const MEASURE_KNOB = "--kp-measure";
/**
 * The knob a theme sets to have the pointer's position written to the page
 * [scope-16]: `--kp-pointer: track`.
 *
 * The spectral instrument's approved demo paints its oxide film as a conic
 * gradient whose start angle follows the pointer — anodising does not add
 * pigment, it grows a film whose thickness decides which wavelength
 * survives, so the colour really does shift with the angle you look from.
 * A gradient cannot read a pointer; something has to write the number down.
 *
 * Off by default, and off under reduced motion: someone asking for less
 * movement is not asking for a colour that follows their hand. The two
 * properties keep whatever the stylesheet declared, so the gradient is
 * valid before the pointer has ever moved and stays valid afterwards.
 */
export declare const POINTER_KNOB = "--kp-pointer";
/** The properties `POINTER_KNOB` drives, each 0 to 1 across the viewport. */
export declare const POINTER: Readonly<{
  x: "--kp-px";
  y: "--kp-py";
}>;
/**
 * The knob a theme sets on the surfaces the pointer LIGHTS [scope-101,
 * from scope-25]: `--kp-light: pointer`.
 *
 * Kenny's sentence for the shade pair, verbatim: "the pointer is the
 * light, and the light half throws its shade away from it while the dark
 * half is lifted out of shade by it". A shadow's direction depends on
 * where its element is, which the two root numbers `POINTER_KNOB` writes
 * cannot say — so this is written per element instead of per page. It
 * rides on that same bus: the same `--kp-pointer: track` arms it, the
 * same `pointermove` listener feeds it, the same animation frame writes
 * both. Off wherever the bus is off, which includes reduced motion.
 */
export declare const LIGHT_KNOB = "--kp-light";
/**
 * The six properties `LIGHT_KNOB` drives on each lit element: the
 * direction away from the pointer (`x`, `y`), how near it is (`near`,
 * `lift`) and where the pointer sits inside the element's own box
 * (`atX`, `atY`). A register declares its fallback for every one of them,
 * so a page with no pointer paints the fixed light it painted before.
 */
export declare const LIGHT: Readonly<{
  x: "--kp-light-x";
  y: "--kp-light-y";
  near: "--kp-light-near";
  lift: "--kp-light-lift";
  atX: "--kp-light-at-x";
  atY: "--kp-light-at-y";
}>;
/** What the light can fall on, the approved demo's own list. */
export declare const LIGHT_SELECTOR =
  ".kp-card, .kp-button:not([class*='kp-button--']), [data-kp-surface='hero']";
/** Past this many pixels the shade is at full length; under it, shorter. */
export declare const LIGHT_REACH = 240;
/** Past this many pixels the light no longer reaches the surface at all. */
export declare const LIGHT_FAR = 560;
/**
 * The knob a theme sets to have the point a press started at written to the
 * button it started on [scope-25, built at scope-101]: `--kp-press: point`.
 *
 * The removed Sepia theme's approved gesture was the ink spreading into the paper on a press,
 * and ink spreads from where the nib touched down, not from the middle of
 * the plate. CSS knows a button is being pressed; it cannot know WHERE, so
 * something has to write the two numbers down. That is all this does — the
 * whole gesture is the register's, and this is the coordinate it reads.
 *
 * Unlike `POINTER_KNOB` it stays armed under reduced motion: someone asking
 * for less movement is not asking for the stain to appear in the wrong
 * place, and the register gives them the same stain with no transition.
 *
 * Without the module, on a key press, or after `detach()`, the two
 * properties are whatever the stylesheet declared — the removed Sepia theme's default was
 * the middle of the button, so the gesture is whole before a pointer has
 * ever touched it [KT6].
 */
export declare const PRESS_KNOB = "--kp-press";
/** The properties `PRESS_KNOB` drives: the press point inside the button's box. */
export declare const PRESS: Readonly<{
  x: "--kp-press-x";
  y: "--kp-press-y";
}>;
/**
 * The second thing a theme can ask a press for: `--kp-press: size`. The
 * button's measured width, in whole pixels, is written to the element a press
 * started on as this attribute, for a theme that letters the size of what is
 * held (blueprint's dimension below the part prints it with
 * `content: attr(data-kp-press-size)`). CSS cannot print a measured length.
 * Written on a pointer press and on Space or Enter; left on the element after
 * the press so a dimension that fades out keeps its numerals.
 */
export declare const PRESS_SIZE = "data-kp-press-size";
/**
 * The third thing a theme can ask a press for: `--kp-press: hold`. A press
 * the stylesheet plays as an animation on `:active` is cut where the pointer
 * lifts, so a tap shorter than the animation shows part of it (deco's seal,
 * 400 ms). The pressed part carries `data-kp-pressed` for the length of the
 * animation, from the pointer going down or Space or Enter going down, and
 * the register keys the animation on `[data-kp-pressed]` beside `:active`.
 * `--kp-press-hold` (a time, read on the part) sets the length; without it
 * `PRESS_HOLD_MS`. The mark is for the time the animation takes, so a longer
 * press is still `:active`'s.
 */
export declare const PRESS_HOLD = "--kp-press-hold";
/** The parts a `hold` press marks: every part a register presses with a sink and a seal. */
export declare const PRESS_HELD =
  ".kp-button, .kp-icon-button, .kp-menu__item, a.kp-kpi, .kp-kpi--toggle, .kp-kpi--trend, .kp-calendar__day";
/** How long a `hold` press stays marked when the theme names no time. */
export declare const PRESS_HOLD_MS = 400;
/** Set on the root before first paint; the register keys its start states on it [AR34]. */
export declare const ROOT_ATTRIBUTE = "data-kp-effects";
/** Set on the root once the reveals of a load have run. */
export declare const DONE_ATTRIBUTE = "data-kp-effects-done";
/**
 * The state a reveal is in, on the element that carries it [TF2, 2026-09-09].
 *
 * `DONE_ATTRIBUTE` says the module has finished the page; this says what
 * happened to one element, and it stays readable afterwards. Until now the
 * only signal was the `kp-reveal` event, which is a moment: whoever was
 * not listening when it fired could never learn the answer. A consumer
 * asking "is the dossier armed yet" had nowhere to look, and a test had
 * nothing to wait for — which is how two retro tests could fail under
 * load while passing alone.
 *
 * Three values, and they are the whole truth about an element:
 *   armed  — wired to a trigger and waiting for it; nothing has run
 *   rest   — settled without playing (reduced motion, no routine, seen)
 *   played — the routine ran
 */
export declare const REVEAL_STATE = "data-kp-reveal-state";
/** The copy of a headline the register's slice pseudo-elements read. */
export declare const TEXT_ATTRIBUTE = "data-kp-text";
/**
 * Dispatched once per page on an element that names a surface or a
 * reveal the vocabulary does not know, bubbling. detail:
 * `{ hook, value, accepted }`. Never thrown [AR44].
 */
export declare const UNKNOWN_EVENT = "kp-effect-unknown";
/**
 * Dispatched on an element when its reveal has reached its rest state,
 * bubbling. detail: `{ reveal, routine, skipped }` — `skipped` says the
 * element went straight to rest (reduced motion, a quiet theme, or seen
 * this session) rather than through the motion.
 */
export declare const REVEAL_EVENT = "kp-reveal";
/** The sessionStorage key prefix of the once-per-session memo [AR44, M4]. */
export declare const MEMO_PREFIX = "kp-effects:";
/** The glyphs a headline deciphers through [AR40]: no block glyphs. */
export declare const GLYPHS = "01<>/\\|=+*#%@&$?!ZXKQ";
/**
 * DI5 for every animation the package runs [TH129, T20, AR40].
 *
 * One row per keyframe name: how long, how often, what moves, and the
 * opacity at each keyframe step (the luminance the gate rates). The
 * register's keyframes carry no comment of their own; this is the table
 * and gates/check-motion.mjs refuses a keyframe without a row or a row
 * whose steps drift from its keyframe. Rates are reported, never
 * corrected (S42).
 *
 * @type {Readonly<Record<string, { durationMs: number, cycles: number, property: string, luminanceSteps: number[] }>>}
 */
export declare const TIMINGS: Readonly<
  Record<
    string,
    {
      durationMs: number;
      cycles: number;
      property: string;
      luminanceSteps: number[];
    }
  >
>;
export type EffectsOptions = {
  /**
   * override the media query (a test, or a consumer's own switch)
   */
  reduceMotion?: boolean;
  /**
   * IntersectionObserver ratio for the rule (default: `--kp-reveal-threshold`, 0.6)
   */
  threshold?: number;
  /**
   * decipher characters per second (default: `--kp-decipher-cps`, 26)
   */
  cps?: number;
  /**
   * ms between one mark clearing and the next (default: `--kp-reveal-stagger`, 260)
   */
  stagger?: number;
  /**
   * ms before the first mark clears on load (default: `--kp-classified-delay`, 1500)
   */
  delay?: number;
  /**
   * set and, on detach, remove the root attribute (default true; a wrapper around one element passes false)
   */
  manageRoot?: boolean;
};
export type EffectsHandle = {
  /**
   * stop everything the module started and remove the root attribute; safe to call twice
   */
  detach: () => void;
  /**
   * start an element rendered after attach, and its subtree [AR34]
   */
  observe: (element: Element) => void;
  /**
   * resolves once every hook this attach asked for has been fetched and run [scope-117]
   */
  ready: Promise<void>;
};
export type EffectsState = {
  detached: boolean;
  io: IntersectionObserver | null;
  ioHeadline: IntersectionObserver | null;
  pending: number;
};
export type EffectsContext = Record<string, any> & {
  state: EffectsState;
};
/** What has been reported as unknown on this page, for js/diagnostics.js. */
export declare function unknownEffects(): any[];
/**
 * Attach the reveals under `root` and return a handle.
 *
 * @param {Document | Element} [root]
 * @param {EffectsOptions} [options]
 * @returns {EffectsHandle}
 */
export declare function attachEffects(
  root?: Document | Element,
  options?: EffectsOptions,
): EffectsHandle;
