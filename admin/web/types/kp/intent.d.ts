/** The most rows a list shows at once; the rest wait for more words (homelab's tested value). */
export declare const INTENT_MAX_SHOWN = 40;
export type IntentOptions = {
  foldDiacritics?: boolean;
  splitHyphen?: boolean;
  typo?: 0 | 1 | boolean;
};
/**
 * The words of a text, each with where it stands in the text as written (so a match can be marked).
 *
 * @param {string} text
 * @param {IntentOptions} [options]
 * @returns {{ w: string, start: number, end: number }[]}
 */
export declare function intentTokens(
  text: string,
  { foldDiacritics, splitHyphen }?: IntentOptions,
): {
  w: string;
  start: number;
  end: number;
}[];
/**
 * The searchable words of a text: lower case, accents folded, hyphenated words also split.
 * @param {string} text
 * @param {IntentOptions} [options]
 * @returns {string[]}
 */
export declare const intentWords: (
  text: string,
  options?: IntentOptions,
) => string[];
/**
 * The words a person typed.
 * @param {string} query
 * @param {IntentOptions} [options]
 * @returns {string[]}
 */
export declare function queryWords(
  query: string,
  { foldDiacritics }?: IntentOptions,
): string[];
/**
 * Whether a typed word starts a word of the text (or, with `typo`, almost does).
 * @param {string} have a word of the text
 * @param {string} want a typed word
 * @param {IntentOptions} [options]
 */
export declare function startsWord(
  have: string,
  want: string,
  { typo }?: IntentOptions,
): boolean;
/**
 * Whether every typed word starts one of the text's words, in any order. An empty query matches everything.
 * @param {string} text
 * @param {string} query
 * @param {IntentOptions} [options]
 */
export declare function matchIntent(
  text: string,
  query: string,
  options?: IntentOptions,
): boolean;
export type IntentItem = {
  /**
   * the words that are shown
   */
  label: string;
  /**
   * more words that are shown (a description); they weigh 1
   */
  hint?: string;
  /**
   * words that are never shown (synonyms); they weigh 1
   */
  words?: string;
};
/**
 * @typedef {object} IntentItem
 * @property {string} label the words that are shown
 * @property {string} [hint] more words that are shown (a description); they weigh 1
 * @property {string} [words] words that are never shown (synonyms); they weigh 1
 */
/**
 * How well an item answers the typed words; -1 when it does not. A word found in the label counts 4 (one more when it
 * is the exact word, two more when it is the first typed word starting the label's first word); one found in the hint
 * or the hidden words counts 1.
 *
 * @param {IntentItem} item
 * @param {string[]} want the typed words (queryWords())
 * @param {IntentOptions} [options]
 */
export declare function intentScore(
  item: IntentItem,
  want: string[],
  options?: IntentOptions,
): number;
/**
 * Where the typed words start words of a text, as ranges of the text as written, merged and in order: what a list
 * marks so the reader sees why a row matched.
 *
 * @param {string} text
 * @param {string[]} want
 * @param {IntentOptions} [options]
 * @returns {[number, number][]}
 */
export declare function intentRanges(
  text: string,
  want: string[],
  options?: IntentOptions,
): [number, number][];
export type IntentCommand = {
  id: string;
  /**
   * the section it is listed under
   */
  group?: string;
  label: string;
  hint?: string;
  words?: string;
  /**
   * the subject it is about, so the open subject's commands come first
   */
  context?: string;
  /**
   * shown before anything is typed
   */
  featured?: boolean;
};
export type RankOptions = {
  /**
   * the subject that is open now: its commands rank higher (+3) and show first
   */
  context?: string | null;
  /**
   * the most rows shown (default 40)
   */
  maxShown?: number;
  /**
   * the sections in the order they are shown (default: the order they first appear)
   */
  groupOrder?: readonly string[];
  /**
   * the ids of the commands run last, newest first: shown first when nothing is typed
   */
  recent?: readonly string[];
  foldDiacritics?: boolean;
  splitHyphen?: boolean;
  typo?: 0 | 1 | boolean;
};
/**
 * @typedef {object} IntentCommand
 * @property {string} id
 * @property {string} [group] the section it is listed under
 * @property {string} label
 * @property {string} [hint]
 * @property {string} [words]
 * @property {string} [context] the subject it is about, so the open subject's commands come first
 * @property {boolean} [featured] shown before anything is typed
 */
/**
 * @typedef {object} RankOptions
 * @property {string | null} [context] the subject that is open now: its commands rank higher (+3) and show first
 * @property {number} [maxShown] the most rows shown (default 40)
 * @property {readonly string[]} [groupOrder] the sections in the order they are shown (default: the order they first appear)
 * @property {readonly string[]} [recent] the ids of the commands run last, newest first: shown first when nothing is typed
 * @property {boolean} [foldDiacritics]
 * @property {boolean} [splitHyphen]
 * @property {0 | 1 | boolean} [typo]
 */
/**
 * The rows for a query: only the commands that answer it, grouped, the best answer first in each group (the open
 * subject's before every other's), at most `maxShown`. With nothing typed it shows what is featured, what is about
 * the open subject and what was run last; when a list has none of those it shows every command, in its own order.
 *
 * @template {IntentCommand} T
 * @param {readonly T[]} commands
 * @param {string} query
 * @param {RankOptions} [options]
 * @returns {{ group: string, commands: T[] }[]}
 */
export declare function rankCommands<T extends IntentCommand>(
  commands: readonly T[],
  query: string,
  { context, maxShown, groupOrder, recent, ...match }?: RankOptions,
): {
  group: string;
  commands: T[];
}[];
/**
 * What a list says when nothing answers: when the first word is a verb it knows, which subjects that verb works on
 * ("There is no stack called jellyfin. Update works on: admin, gateway."), otherwise what to try. The words are the
 * dictionary's (`intentNoSubject`, `intentNoMatch`).
 *
 * @param {string} query
 * @param {{ verbs?: readonly string[], subjects?: readonly string[], noun?: string, hint?: string }} [known]
 * @param {IntentOptions} [options]
 * @returns {string}
 */
export declare function noMatchText(
  query: string,
  {
    verbs,
    subjects,
    noun,
    hint,
  }?: {
    verbs?: readonly string[];
    subjects?: readonly string[];
    noun?: string;
    hint?: string;
  },
  options?: IntentOptions,
): string;
