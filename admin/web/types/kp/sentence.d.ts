/** A sentence form. */
export declare const SENTENCE = "[data-kp-sentence]";
/** Fired on the sentence, bubbling, after a change, once things have settled: `{ summary }`. */
export declare const SENTENCE_EVENT = "kp-sentence-change";
/**
 * The sentence in words as it stands now: the text, with each field replaced by what it shows and each hidden phrase left out.
 * @param {Element} element a `.kp-sentence`
 * @returns {string}
 */
export declare function sentenceSummary(element: Element): string;
/**
 * Mark a control invalid and write the line under the sentence that names it; `why` null clears the mark.
 * @param {Element} element a `.kp-sentence`
 * @param {string} name the control's `name`
 * @param {string | null} why
 */
export declare function setSentenceInvalid(
  element: Element,
  name: string,
  why: string | null,
): void;
/**
 * Write (or clear, with null) the line under the sentence.
 * @param {Element} element a `.kp-sentence` @param {string | null} text
 */
export declare function setSentenceWhy(
  element: Element,
  text: string | null,
): void;
/**
 * Put a field in a state: `loading` shows its own skeleton at the width it had and switches its control off; `ready` brings it back.
 * @param {Element} field a `.kp-sentence__field`
 * @param {'loading' | 'ready'} state
 */
export declare function setSentenceFieldState(
  field: Element,
  state: "loading" | "ready",
): void;
export type SentenceHandle = {
  element: HTMLElement;
  /**
   * the line under the sentence
   */
  line: HTMLElement;
  /**
   * the sentence in words
   */
  summary: () => string;
  /**
   * show and hide the phrases and place the fields again
   */
  refresh: () => void;
};
/** The handle of an attached sentence. @param {Element} element */
export declare function sentence(element: Element): SentenceHandle | null;
/**
 * Wire every sentence form under `root`.
 * @param {ParentNode} [root]
 * @returns {() => void} detach
 */
export declare function attachSentences(root?: ParentNode): () => void;
