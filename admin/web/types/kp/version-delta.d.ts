// The kp-themes module chassis-rs serves at /static/kp/js/version-delta.js (kp-themes 11.1.1),
// copied from kp-themes js/version-delta.d.ts: chassis serves the module, not its types.
/** The chips this module draws. */
export declare const VERSION_DELTA =
  "[data-kp-version-to], [data-kp-version-from], .kp-badge--version[data-kp-state]";
/** Fired on the chip after Shift+click copied it: `{ text }`. */
export declare const VERSION_COPY_EVENT = "kp-version-copy";
/** The levels a jump has, loudest last in a sort. */
export declare const VERSION_LEVELS: readonly [
  "same",
  "other",
  "patch",
  "minor",
  "major",
  "downgrade",
  "unknown",
];
/** The sort value of each level: the risky first when sorted down. */
export declare const VERSION_SORT: Readonly<{
  downgrade: 4;
  major: 3;
  minor: 2;
  patch: 1;
  other: 0;
  same: 0;
  unknown: 0;
}>;
export type VersionLevel =
  "same" | "other" | "patch" | "minor" | "major" | "downgrade" | "unknown";
/** @typedef {'same' | 'other' | 'patch' | 'minor' | 'major' | 'downgrade' | 'unknown'} VersionLevel */
/**
 * The numbers in a version, in order: `v10.11.11` gives [10, 11, 11], linuxserver's `5.2.3_v2.0.14-ls474` gives
 * [5, 2, 3, 2, 0, 14, 474]. The same reading the host's pin check uses.
 * @param {string} version
 * @returns {number[]}
 */
export declare function versionNumbers(version: string): number[];
/**
 * `owner/repo` of an upstream as the fleet check names it (`github.com/owner/repo`, with a scheme, `www.` or `.git`
 * allowed); null for anything that is not a GitHub repository.
 * @param {string} upstream
 * @returns {{ owner: string, repo: string } | null}
 */
export declare function githubRepo(upstream: string): {
  owner: string;
  repo: string;
} | null;
/**
 * The newer version's release page, absolute; null when the upstream is not a GitHub repository or there is no tag.
 * @param {string} upstream
 * @param {string} latest
 * @returns {string | null}
 */
export declare function releaseUrl(
  upstream: string,
  latest: string,
): string | null;
export type LevelOptions = {
  scheme?: "numbers" | "semver" | "calver" | "none";
  /**
   * which number's change is a big jump
   */
  bigAt?: "first" | "second";
  /**
   * the app's name
   */
  app?: string;
  /**
   * apps whose second number counts as big too
   */
  bigMinorApps?: readonly string[];
  /**
   * one app's own say
   */
  level?: VersionLevel | "auto";
};
/**
 * @typedef {object} LevelOptions
 * @property {'numbers' | 'semver' | 'calver' | 'none'} [scheme]
 * @property {'first' | 'second'} [bigAt] which number's change is a big jump
 * @property {string} [app] the app's name
 * @property {readonly string[]} [bigMinorApps] apps whose second number counts as big too
 * @property {VersionLevel | 'auto'} [level] one app's own say
 */
/**
 * How big a jump is: `major` (the first number rose, or the second for an app that asks for it), `minor` (the second),
 * `patch` (any later number), `downgrade` (a number fell), `same`, `other` (something changed that has no numbers in
 * it, such as a digest) or `unknown` (a version is missing, or the scheme does not read numbers).
 *
 * @param {string | null | undefined} from
 * @param {string | null | undefined} to
 * @param {LevelOptions} [options]
 * @returns {VersionLevel}
 */
export declare function versionLevel(
  from: string | null | undefined,
  to: string | null | undefined,
  { scheme, bigAt, app, bigMinorApps, level }?: LevelOptions,
): VersionLevel;
/**
 * The level of each jump of a list counted exactly, and the words for a heading ("2 major, 5 minor, 3 patch").
 * @param {readonly { from?: string | null, to?: string | null, app?: string, level?: VersionLevel | 'auto' }[]} list
 * @param {LevelOptions} [options]
 * @returns {{ total: number, counts: Record<VersionLevel, number>, text: string }}
 */
export declare function summarizeDeltas(
  list: readonly {
    from?: string | null;
    to?: string | null;
    app?: string;
    level?: VersionLevel | "auto";
  }[],
  options?: LevelOptions,
): {
  total: number;
  counts: Record<VersionLevel, number>;
  text: string;
};
/**
 * Set a chip's versions and draw it again.
 * @param {HTMLElement} el
 * @param {{ from?: string | null, to?: string | null, app?: string, href?: string | null, state?: 'loading' | 'error' | 'empty' | 'ready', error?: string }} change
 */
export declare function setVersionDelta(
  el: HTMLElement,
  {
    from,
    to,
    app,
    href,
    state,
    error,
  }?: {
    from?: string | null;
    to?: string | null;
    app?: string;
    href?: string | null;
    state?: "loading" | "error" | "empty" | "ready";
    error?: string;
  },
): void;
/**
 * Draw every version chip under `root` and keep it in step with its attributes.
 * @param {ParentNode} [root]
 * @param {{ bigMinorApps?: readonly string[], href?: string | null }} [options]
 * @returns {() => void} detach
 */
export declare function attachVersionDeltas(
  root?: ParentNode,
  options?: {
    bigMinorApps?: readonly string[];
    href?: string | null;
  },
): () => void;
