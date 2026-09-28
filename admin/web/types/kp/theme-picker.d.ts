// The kp-themes module chassis-rs serves at /static/kp/js/theme-picker.js (kp-themes 7.2.0),
// copied from kp-themes js/theme-picker.d.ts: chassis serves the module, not its types.
import { THEMES } from "./theme-core.js";
/** Dispatched on the picker, bubbling, when a person chose a theme here: `{ theme, stored }`. */
export declare const PICK_EVENT = "kp-theme-pick";
/**
 * Attach the behaviour to every picker under `root`, and keep them all in
 * step with each other and with any React picker on the same page — they
 * share one bus, so neither channel needs to know the other exists [AR5].
 *
 * Safe to call twice: a picker already attached is skipped. The returned
 * detach restores the marks it made; `refresh()` re-marks options added
 * after attach, which the idempotency guard would otherwise skip.
 *
 * @param {ParentNode} [root]
 * @param {{ persist?: boolean, closePopover?: boolean, status?: ParentNode | null }} [options]
 *   persist: store the choice (default true); closePopover: close an
 *   enclosing popover after a choice (default true); status: where the
 *   save-failed message lives (default: the picker's parent).
 * @returns {(() => void) & { refresh: () => void }} detach
 */
export declare function attachThemePickers(
  root?: ParentNode,
  {
    persist,
    closePopover,
    status,
  }?: {
    persist?: boolean;
    closePopover?: boolean;
    status?: ParentNode | null;
  },
): (() => void) & {
  refresh: () => void;
};
/**
 * The list a consumer's template needs to render the options. Exported so
 * a page built without a server-side theme list can still write the
 * markup, and so the showcase does not hand-type seven names.
 */
export { THEMES };
/** The icon the menu button wears by default; pass your own SVG string to `icon`. */
export declare const THEME_MENU_ICON: string;
/**
 * The options of a picker, as markup — grouped light and dark [TH63].
 *
 * Each group is a `<li role="presentation">` carrying a small heading
 * and its own list, so the menu reads "Light: Formal, Light, … — Dark:
 * Dark, Cyberpunk, …" rather than eleven names in one run. The grouping
 * comes from the registry's `dark` flag, which is generated from the
 * tokens, so this cannot disagree with the stylesheet about which is
 * which. Pass `grouped: false` for one flat list.
 *
 * @param {{ themes?: readonly import('./theme-registry.js').ThemeRecord[], labels?: Partial<Record<string, string>>, grouped?: boolean, groupLabels?: { light?: string, dark?: string } }} [options]
 * @returns {string}
 */
export declare function themeOptionsMarkup({
  themes,
  labels,
  grouped,
  groupLabels,
}?: {
  themes?: readonly import("./theme-registry.js").ThemeRecord[];
  labels?: Partial<Record<string, string>>;
  grouped?: boolean;
  groupLabels?: {
    light?: string;
    dark?: string;
  };
}): string;
/**
 * The icon button with a dropdown, as markup [S2].
 *
 * Returned as a string rather than rendered, so a server can print it
 * into a template and a page can insert it — the same choice T1 makes
 * everywhere else in this channel. The id must be unique on the page;
 * pass one when there are two menus. Everything a person reads is
 * escaped; the icon is trusted markup you pass.
 *
 * @param {{ id?: string, label?: string, icon?: string, themes?: readonly import('./theme-registry.js').ThemeRecord[], labels?: Partial<Record<string, string>>, grouped?: boolean, groupLabels?: { light?: string, dark?: string }, className?: string }} [options]
 * @returns {string}
 */
export declare function themeMenuMarkup({
  id,
  label,
  icon,
  themes,
  labels,
  grouped,
  groupLabels,
  className,
}?: {
  id?: string;
  label?: string;
  icon?: string;
  themes?: readonly import("./theme-registry.js").ThemeRecord[];
  labels?: Partial<Record<string, string>>;
  grouped?: boolean;
  groupLabels?: {
    light?: string;
    dark?: string;
  };
  className?: string;
}): string;
