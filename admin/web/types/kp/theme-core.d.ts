import { THEMES, DEFAULT_THEME, STORAGE_KEY } from "./theme-registry.js";
export type ThemeName = import("./theme-registry.js").ThemeName;
/**
 * A theme name, as a union of the eleven that exist [KT4].
 *
 * Only the OUTPUTS of this module narrowed to it. What a function accepts
 * stays lenient — `storeTheme` and `initializeTheme` still take a plain
 * string — because narrowing an input breaks a consumer that reads a theme
 * out of config or a database, which is exactly what JobTracker and
 * kp-soft do. Narrowing a return value cannot break anyone.
 *
 * @typedef {import('./theme-registry.js').ThemeName} ThemeName
 */
/**
 * The event both channels listen to. A contract value: a consumer may
 * listen for it too, so it does not get renamed casually [TH26]. It
 * bubbles from the root, so `document.addEventListener` sees it as it
 * always did, and so does a listener on the root itself.
 */
export declare const THEME_EVENT = "kp-theme-change";
/** Fired before a change, cancelable: `{ theme, previous }`. preventDefault() keeps the current theme. */
export declare const BEFORE_THEME_EVENT = "kp-theme-before-change";
/**
 * Fired when a name was dropped: `{ requested, applied, source }` [TH97].
 *
 * `source` is one of `stored`, `current`, `apply` or `cross-tab` — the
 * four places a name can be dropped (AR25). A consumer listening to this
 * learns what its page asked for and what it got, which is the whole of
 * what the silence used to cost.
 */
export declare const UNKNOWN_THEME_EVENT = "kp-theme-unknown";
/**
 * The theme a change is being HELD for, on the root [scope-50].
 *
 * A listener on BEFORE_THEME_EVENT can say no for two reasons, and a
 * caller needs to tell them apart: a refusal (the change will not happen)
 * and a hold (it will, once something has arrived — js/lazy-register.js
 * holds a change until the new theme's register has loaded, because
 * tokens without their register are a theme that is not that theme).
 * A holding listener writes the requested name here before it cancels;
 * `applyTheme()` removes it whenever a theme is actually put on the root,
 * so a later choice supersedes a held one. `pendingTheme()` reads it.
 */
export declare const PENDING_THEME_ATTRIBUTE = "data-theme-pending";
/**
 * The theme a root is really wearing while it shows a PREVIEW of another, on the root [anchors-picker-1019].
 *
 * `previewTheme()` puts a theme on the root to look at it, without a change: nothing is announced, nothing is stored, no
 * listener is asked. While it is set, `data-theme` is the previewed theme and this attribute names the real one;
 * `currentTheme()` still answers the real one, so a picker's mark, a stored choice and every `kp-theme-change` listener keep
 * seeing the theme the person chose. `endPreview()` removes it, and so does `applyTheme()` whenever it puts a theme on the root.
 */
export declare const PREVIEW_THEME_ATTRIBUTE = "data-theme-preview-from";
export type ThemeConfig = {
  root?: Element;
  darkClass?: string | null;
  storageKey?: string;
};
/**
 * Set the defaults once: which element wears the theme (default: the
 * document element), which class marks a dark theme (default `dark`; null
 * for none), and the storage key.
 *
 * @param {ThemeConfig} next
 */
export declare function configureTheme(next: ThemeConfig): void;
/** @param {unknown} value @returns {value is ThemeName} */
export declare const isTheme: (value: unknown) => value is ThemeName;
/**
 * @param {{ root?: Element }} [options]
 * @returns {ThemeName} the theme the root is currently wearing
 */
export declare function currentTheme({ root }?: { root?: Element }): ThemeName;
/**
 * The theme a change is being held for, or null when none is [scope-50].
 * While it is set, `currentTheme()` is still the theme the root wears.
 *
 * @param {{ root?: Element }} [options]
 * @returns {ThemeName | null}
 */
export declare function pendingTheme({
  root,
}?: {
  root?: Element;
}): ThemeName | null;
/**
 * Put a theme on the root and tell everyone.
 *
 * Validation lives here rather than in each caller: this is the exported
 * entry point and was the only one that did not validate, which is how an
 * unknown value used to reach the DOM through `applyTheme` while the same
 * value was rejected by the hook (AR6, adopted from the critic).
 *
 * @param {unknown} theme
 * @param {{ root?: Element, darkClass?: string | null, strict?: boolean, announce?: boolean }} [options]
 *   strict: throw on an unknown name instead of substituting the default; announce: dispatch the events (default true)
 * @returns {ThemeName} the theme actually applied — DEFAULT_THEME for anything unknown.
 *   When a listener refused or HELD the change, the theme the root still wears: with
 *   js/lazy-register.js attached that is the previous theme until the new register has
 *   loaded, and `pendingTheme()` names the one on its way [scope-50].
 */
export declare function applyTheme(
  theme: unknown,
  {
    root,
    darkClass,
    strict,
    announce,
  }?: {
    root?: Element;
    darkClass?: string | null;
    strict?: boolean;
    announce?: boolean;
  },
): ThemeName;
/**
 * Show a theme on the root for a look, without choosing it [anchors-picker-1019].
 *
 * Nothing is announced (no `kp-theme-before-change`, no `kp-theme-change`), nothing is stored, and `currentTheme()` goes on
 * answering the theme the root really wears; only `data-theme` and the dark class change, so the page, its charts that read the
 * theme from the root and the tokens all show the previewed theme. A theme that does not exist is ignored (no fallback to the
 * default: a preview of nothing is no preview). Previewing the theme already worn ends a preview. `endPreview()` goes back;
 * `applyTheme()` makes a choice and ends it. A theme whose register is loaded on demand needs it loaded first:
 * js/theme-picker.js does that before it calls this.
 *
 * @param {unknown} theme
 * @param {{ root?: Element, darkClass?: string | null }} [options]
 * @returns {ThemeName | null} the theme now shown, or null when the name is not a theme
 */
export declare function previewTheme(
  theme: unknown,
  {
    root,
    darkClass,
  }?: {
    root?: Element;
    darkClass?: string | null;
  },
): ThemeName | null;
/**
 * Go back from a preview to the theme the root really wears.
 *
 * @param {{ root?: Element, darkClass?: string | null }} [options]
 * @returns {boolean} whether there was a preview to end
 */
export declare function endPreview({
  root,
  darkClass,
}?: {
  root?: Element;
  darkClass?: string | null;
}): boolean;
/**
 * The theme being previewed on the root, or null when none is.
 *
 * @param {{ root?: Element }} [options]
 * @returns {ThemeName | null}
 */
export declare function previewedTheme({
  root,
}?: {
  root?: Element;
}): ThemeName | null;
/**
 * Remember the choice. Returns false when storage refused — private mode,
 * blocked storage, a full quota. The caller shows that; it is not swallowed
 * [AR6], because in a server-rendered dashboard a preference that silently
 * fails to save is indistinguishable from a broken picker.
 *
 * @param {string} theme
 * @param {{ key?: string, storage?: Storage }} [options]
 * @returns {boolean} whether the choice will survive a reload
 */
export declare function storeTheme(
  theme: string,
  {
    key,
    storage,
  }?: {
    key?: string;
    storage?: Storage;
  },
): boolean;
/**
 * @param {{ key?: string, storage?: Storage }} [options]
 * @returns {ThemeName | null} the stored choice, or null if there is none or storage is unreadable
 */
export declare function storedTheme({
  key,
  storage,
}?: {
  key?: string;
  storage?: Storage;
}): ThemeName | null;
/**
 * Before anything renders: wear the last known choice. Six lines in a
 * consumer's <head>, deliberately ignorant of which themes are dark —
 * that knowledge lives in the generated registry [TH23].
 *
 * @param {string} [fallback]
 * @param {{ root?: Element, key?: string }} [options]
 * @returns {ThemeName}
 */
export declare function initializeTheme(
  fallback?: string,
  {
    root,
    key,
  }?: {
    root?: Element;
    key?: string;
  },
): ThemeName;
/**
 * Listen for theme changes, whoever made them: this tab's React picker,
 * this tab's framework-free picker, or — unless declined — another tab.
 *
 * @param {(theme: ThemeName, detail: { previous: ThemeName | null, root: Element }) => void} listener
 * @param {{ crossTab?: boolean, root?: Element, key?: string }} [options]
 * @returns {() => void} unsubscribe
 */
export declare function onThemeChange(
  listener: (
    theme: ThemeName,
    detail: {
      previous: ThemeName | null;
      root: Element;
    },
  ) => void,
  {
    crossTab,
    root,
    key,
  }?: {
    crossTab?: boolean;
    root?: Element;
    key?: string;
  },
): () => void;
export { THEMES, DEFAULT_THEME, STORAGE_KEY };
