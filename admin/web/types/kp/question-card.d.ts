/** A container of questions. */
export declare const QUESTIONS = "[data-kp-questions]";
/** Fired when an answer was chosen. */
export declare const QUESTION_ANSWER_EVENT = "kp-question-answer";
/** Fired when the time ran out. */
export declare const QUESTION_EXPIRE_EVENT = "kp-question-expire";
/** Fired when a finished card was dismissed. */
export declare const QUESTION_DISMISS_EVENT = "kp-question-dismiss";
/** Seconds before the end from which a card is urgent. */
export declare const QUESTION_URGENT_AT = 30;
export type QuestionAnswer = {
  value: string;
  label: string;
  /**
   * what this answer sets in motion
   */
  consequence?: string;
  /**
   * what the card says once it is chosen and sent
   */
  outcome?: string;
  /**
   * a destructive answer is drawn as one
   */
  tone?: "destructive" | "primary" | "";
};
export type QuestionSpec = {
  id: string;
  question: string;
  text?: string;
  /**
   * ISO or epoch ms
   */
  deadline: number | string;
  asked?: number | string;
  /**
   * default: Allow and Stop
   */
  answers?: QuestionAnswer[];
  defaultAnswer?: string;
  urgentAt?: number;
  expired?: "keep" | "remove";
  countdown?: "bar" | "ring" | "text";
};
export type QuestionHandle = {
  element: HTMLElement;
  id: string;
  state: () => string;
  /**
   * choose an answer as if the person had
   */
  answer: (value: string) => void;
  dismiss: () => void;
};
/** The handle of a card. @param {Element} element */
export declare const questionOf: (element: Element) =>
  | (QuestionHandle & {
      stop: () => void;
    })
  | null;
/**
 * Ask a question: put a card in `container` (oldest first).
 * @param {HTMLElement} container a `[data-kp-questions]` (or any element)
 * @param {QuestionSpec} spec
 * @param {{ now?: () => number, frozen?: boolean }} [options] `now` is the clock; `frozen` stops the countdown's motion and its tick (a still demonstration)
 * @returns {QuestionHandle}
 */
export declare function askQuestion(
  container: HTMLElement,
  spec: QuestionSpec,
  {
    now,
    frozen,
  }?: {
    now?: () => number;
    frozen?: boolean;
  },
): QuestionHandle;
/**
 * Make every `[data-kp-questions]` container a labelled region; the cards are made with askQuestion().
 * @param {ParentNode} [root]
 * @returns {() => void} detach: stops every card's clock
 */
export declare function attachQuestions(root?: ParentNode): () => void;
