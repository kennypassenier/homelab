// Live view pace (Kenny, 2026-09-29 ~18:14: "selecties in een dropdown
// mogen veel trager, typen mag iets trager, het moet een beetje een mens
// nadoen"). Every timing of Claude's driven steps in a tab that follows
// lives here as a named constant, with the pure functions that turn them
// into a plan: how long after each typed letter, and which options a
// dropdown pick passes over and how long it rests on each. Pure (no DOM,
// no clock); the randomness is injected so tests are deterministic.
// Tested in test/pace.test.js.

/** @typedef {() => number} Rand a source of numbers in [0, 1) */

// ── typing ─────────────────────────────────────────────────────────────

/** Shortest wait after a typed letter (ms). */
export const TYPE_MIN_MS = 90;
/** Longest wait after a typed letter, before pauses (ms). */
export const TYPE_MAX_MS = 180;
/** Extra rest after a space: the gap between words (ms, min and max). */
export const TYPE_SPACE_PAUSE_MS = [60, 160];
/** Extra rest after punctuation (ms, min and max). */
export const TYPE_PUNCT_PAUSE_MS = [180, 360];
/** A long text never takes longer than this in all (ms). */
export const TYPE_MAX_TOTAL_MS = 8000;
/** A letter never comes faster than this, even when squeezed (ms). */
export const TYPE_FLOOR_MS = 12;
/** How long the glide to a field takes before Claude types (ms). */
export const TYPE_GLIDE_MS = 350;
/** Reduced motion: the whole text lands at once, then this rest (ms). */
export const REDUCED_TYPE_MS = 120;

/** Letters a person pauses after. */
const PUNCT = new Set([".", ",", ";", ":", "!", "?", "/", "-", ")"]);

/**
 * A value between `lo` and `hi` from the injected source.
 * @param {Rand} rand
 * @param {number} lo
 * @param {number} hi
 */
const between = (rand, lo, hi) =>
  lo + (hi - lo) * Math.max(0, Math.min(1, rand()));

/**
 * How long to wait after each letter of `text`, one number per letter:
 * a little uneven, a rest after a word and after punctuation, and scaled
 * down as a whole when the text would take more than TYPE_MAX_TOTAL_MS.
 * Reduced motion: an empty plan (the caller sets the whole text at once).
 * @param {string} text
 * @param {Rand} [rand]
 * @param {boolean} [reduced]
 * @returns {number[]} milliseconds, rounded
 */
export function typingDelays(text, rand = Math.random, reduced = false) {
  if (reduced) return [];
  const letters = [...text];
  const raw = letters.map((ch) => {
    let d = between(rand, TYPE_MIN_MS, TYPE_MAX_MS);
    if (ch === " " || ch === "\n")
      d += between(rand, TYPE_SPACE_PAUSE_MS[0], TYPE_SPACE_PAUSE_MS[1]);
    else if (PUNCT.has(ch))
      d += between(rand, TYPE_PUNCT_PAUSE_MS[0], TYPE_PUNCT_PAUSE_MS[1]);
    return d;
  });
  const sum = raw.reduce((a, b) => a + b, 0);
  const scale = sum > TYPE_MAX_TOTAL_MS ? TYPE_MAX_TOTAL_MS / sum : 1;
  return raw.map((d) => Math.round(Math.max(TYPE_FLOOR_MS, d * scale)));
}

// ── picking from a dropdown ────────────────────────────────────────────

/** The glide from wherever the pointer is to the dropdown (ms). */
export const PICK_GLIDE_MS = 650;
/** Rest after the list opened, reading it (ms). */
export const PICK_OPEN_PAUSE_MS = 700;
/** The move from one option to the next (ms). */
export const PICK_STEP_MOVE_MS = 180;
/** A brief hover on each option passed over (ms, min and max). */
export const PICK_HOVER_MS = [140, 260];
/** At most this many options are passed over; a longer way skips some. */
export const PICK_MAX_HOVERS = 7;
/** Rest on the chosen option before it is clicked (ms). */
export const PICK_CHOICE_PAUSE_MS = 650;
/** How long the list stays after the click, the choice marked (ms). */
export const PICK_CLOSE_MS = 250;
/** Reduced motion: the value is set at once, then this rest (ms). */
export const REDUCED_PICK_MS = 200;

/**
 * Which options the pointer passes over on the way from the option the
 * list opened on (`from`, -1 for none) to the chosen one (`to`), the
 * chosen one last. A long way keeps at most PICK_MAX_HOVERS stops, evenly
 * spread, as a person sweeps past most of a long list.
 * @param {number} from
 * @param {number} to
 * @param {number} [max]
 * @returns {number[]}
 */
export function pickPath(from, to, max = PICK_MAX_HOVERS) {
  if (to < 0) return [];
  const start = from < 0 ? 0 : from;
  if (start === to) return [to];
  const dir = to > start ? 1 : -1;
  /** @type {number[]} */
  const all = [];
  // The option it opened on is where the pointer starts, not a stop.
  for (let i = from < 0 ? start : start + dir; i !== to + dir; i += dir)
    all.push(i);
  const keep = Math.max(1, max);
  if (all.length <= keep) return all;
  /** @type {number[]} */
  const out = [];
  for (let k = 1; k <= keep; k += 1)
    out.push(all[Math.round((k * all.length) / keep) - 1]);
  return out;
}

/**
 * @typedef {{glideMs: number, openPauseMs: number,
 *   hovers: {index: number, moveMs: number, restMs: number}[],
 *   choicePauseMs: number, closeMs: number}} PickPlan
 */

/**
 * The whole pick, step by step. Reduced motion: null (set at once).
 * @param {number} from the option selected when the list opens (-1: none)
 * @param {number} to the chosen option
 * @param {Rand} [rand]
 * @param {boolean} [reduced]
 * @returns {PickPlan | null}
 */
export function pickPlan(from, to, rand = Math.random, reduced = false) {
  if (reduced || to < 0) return null;
  return {
    glideMs: PICK_GLIDE_MS,
    openPauseMs: PICK_OPEN_PAUSE_MS,
    hovers: pickPath(from, to).map((index) => ({
      index,
      moveMs: PICK_STEP_MOVE_MS,
      restMs: Math.round(between(rand, PICK_HOVER_MS[0], PICK_HOVER_MS[1])),
    })),
    choicePauseMs: PICK_CHOICE_PAUSE_MS,
    closeMs: PICK_CLOSE_MS,
  };
}

/**
 * Where the drawn list goes: under the dropdown, or above it when there
 * is no room below inside the safe band (and more room above).
 * @param {{top: number, height: number}} box the dropdown
 * @param {number} listHeight
 * @param {{top: number, bottom: number}} band
 * @returns {number} the list's top (client px)
 */
export function listTop(box, listHeight, band) {
  const below = box.top + box.height;
  const roomBelow = band.bottom - below;
  const roomAbove = box.top - band.top;
  if (roomBelow >= listHeight || roomBelow >= roomAbove) return below;
  return box.top - listHeight;
}
