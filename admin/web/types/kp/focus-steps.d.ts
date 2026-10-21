export declare const FOCUS_STEPS = "[data-kp-focus-steps]";
export declare const FOCUS_STEPS_EVENT = "kp-focus-steps-change";
export declare const FOCUS_STEPS_DONE_EVENT = "kp-focus-steps-done";
export declare const FOCUS_STEPS_WAKE_EVENT = "kp-focus-steps-wake";
/** Fired on the root, cancelable, before an answer is taken: `{ index, id, value, source }`. */
export declare const FOCUS_STEPS_BEFORE_ANSWER_EVENT =
  "kp-focus-steps-before-answer";
/** Fired on the root when a question was answered: `{ index, id, value, source: 'tap' | 'same' | 'skip', same?, previous }`. */
export declare const FOCUS_STEPS_ANSWER_EVENT = "kp-focus-steps-answer";
/** Fired on the root when answers were put back after a break: `{ index, answers }`. */
export declare const FOCUS_STEPS_RESUME_EVENT = "kp-focus-steps-resume";
/** Fired on the root when Retry is pressed on the plate of questions that could not be read. */
export declare const FOCUS_STEPS_RETRY_EVENT = "kp-focus-steps-retry";
/** Fired on the root when the action of the empty plate (`data-kp-focus-steps-empty-action`) is pressed [gaps-time-1019]. */
export declare const FOCUS_STEPS_EMPTY_ACTION_EVENT =
  "kp-focus-steps-empty-action";
/** Fired on the root when Try again is pressed on the plate of answers that could not be saved. */
export declare const FOCUS_STEPS_SAVE_RETRY_EVENT = "kp-focus-steps-save-retry";
/** Fired on the root when the active track changed: `{ track, key, from, index, step, reason }`. */
export declare const FOCUS_STEPS_TRACK_EVENT = "kp-focus-steps-track";
/** Fired on the root when a track asks for the person: `{ track, key, reason: 'timer' | 'ready', index }`. */
export declare const FOCUS_STEPS_TRACK_ATTENTION_EVENT =
  "kp-focus-steps-track-attention";
/** How far a finger has to travel across the step to turn it [px]. */
export declare const SWIPE_DISTANCE = 56;
export type FocusIngredient = {
  name: string;
  amount?: string;
};
export type FocusChoice = {
  value: string;
  label: string;
  disabled?: boolean;
  /**
   * why it cannot be picked, shown under its label
   */
  reason?: string;
};
export type FocusAnswer = {
  /**
   * names the answer in the events and the done payload
   */
  id: string;
  kind: "choice" | "multi" | "text";
  choices?: FocusChoice[];
  /**
   * yesterday's answer: offered as "Same as yesterday"
   */
  previous?: string | string[];
  /**
   * the usual answer: offered as "Same as usual"
   */
  usual?: string | string[];
  /**
   * a text box's hint
   */
  placeholder?: string;
};
export type FocusStep = {
  text: string;
  /**
   * makes the step a question
   */
  answer?: FocusAnswer;
  ingredients?: (FocusIngredient | string)[];
  /**
   * `id` names the timer in the timer list (default: the step's `id`)
   */
  timer?: {
    seconds: number;
    name?: string;
    id?: string;
  };
  /**
   * names the step for `waitFor` and for its timer (default `step-<n>`)
   */
  id?: string;
  /**
   * the line of work it belongs to ("Pasta"); steps with none belong to the shared lane
   */
  track?: string;
  /**
   * a short name for the step, used in the lanes and where another step waits for it
   */
  title?: string;
  /**
   * step ids, or track names, that must be done before this step can start
   */
  waitFor?: string | string[];
  /**
   * the same as `waitFor`
   */
  after?: string | string[];
  note?: string;
  noteTone?: "info" | "warning";
};
export type FocusState = {
  steps: FocusStep[];
  index: number;
  wake: WakeLockSentinel | null;
  wakeState: string;
  busy: import("./busy-card.js").BusyCard | null;
  answers: Record<
    string,
    {
      value: string | string[] | null;
      source: string;
    }
  >;
  /**
   * the taps of a multi step and the words of a text step, before Next
   */
  draft: Record<string, string | string[]>;
  /**
   * the pending advance after a tap
   */
  timer: number;
  startedAt: number;
  /**
   * the lines of work, in the order the steps name them (one only: no lanes)
   */
  tracks: Track[];
  /**
   * the key of the active track
   */
  track: string;
  /**
   * the steps passed with Next
   */
  done: Set<number>;
  /**
   * the tracks asking for the person, by track key
   */
  alerts: Map<string, TrackAlert>;
  /**
   * the tracks that waited when the lanes were last read
   */
  waiting: Set<string>;
  /**
   * the steps the shown step waits for, as a key
   */
  lockKey: string;
  /**
   * the interval that keeps the lanes' clocks, 0 when none
   */
  ticker: number;
  pendingResume: Progress | null;
  finished: {
    total: number;
    answers: FocusState["answers"];
    startedAt: number;
    finishedAt: number;
  } | null;
};
export type Progress = {
  index: number;
  answers: FocusState["answers"];
  startedAt: number;
  /**
   * the key of the active track
   */
  track?: string;
  /**
   * the place on every track, by track key
   */
  positions?: Record<string, number>;
  /**
   * the steps passed
   */
  done?: number[];
};
export type Track = {
  /**
   * the name the steps give it ('' for the shared lane)
   */
  key: string;
  name: string;
  /**
   * the step indexes, in recipe order
   */
  order: number[];
  /**
   * where on the track the person is
   */
  pos: number;
};
export type TrackAlert = {
  reason: "timer" | "ready";
  title: string;
  text: string;
  /**
   * the step the track is on
   */
  index: number;
  timerId?: string;
};
/**
 * What a step waits for and has not got yet: for a step reference that step, for a track reference the first step of it
 * that is not done. A reference nobody carries waits for nothing.
 * @param {FocusState} state @param {number} i @param {import('./timer.js').TimerState[]} timers
 * @returns {{ index: number, whole: boolean, track: Track | undefined }[]}
 */
declare function blockersOf(
  state: FocusState,
  i: number,
  timers: import("./timer.js").TimerState[],
): {
  index: number;
  whole: boolean;
  track: Track | undefined;
}[];
export type Lane = {
  track: Track;
  key: string;
  name: string;
  active: boolean;
  status: "ready" | "running" | "paused" | "waiting" | "ringing" | "finished";
  /**
   * the clock the lane shows
   */
  timer: import("./timer.js").TimerState | null;
  blocks: ReturnType<typeof blockersOf>;
};
/**
 * Go to a step (as a person did: the event fires when it moved).
 * @param {Element} root @param {number} index
 * @returns {boolean} whether it moved
 */
export declare function goToStep(root: Element, index: number): boolean;
/** @param {Element} root */
export declare const getStepIndex: (root: Element) => number;
/**
 * Give the steps, and where to begin (to come back where a person stopped).
 * No event: the page knows.
 * @param {Element} root @param {FocusStep[]} steps @param {{ start?: number }} [options]
 */
export declare function setFocusSteps(
  root: Element,
  steps: FocusStep[],
  {
    start,
  }?: {
    start?: number;
  },
): void;
/**
 * The state: `loading` lays the busy card over the body, `error` says the
 * message where the step was, `ready` is neither.
 * @param {Element} root @param {'loading' | 'error' | 'save-error' | 'ready'} kind @param {string} [message]
 */
export declare function setFocusStepsState(
  root: Element,
  kind: "loading" | "error" | "save-error" | "ready",
  message?: string,
): void;
/**
 * Wire every focus-steps view under `root`: its buttons, keys, swipe, Read
 * aloud, and the screen kept on until the detach.
 * @param {ParentNode} [scope]
 * @returns {() => void} detach
 */
export declare function attachFocusSteps(scope?: ParentNode): () => void;
export {};
