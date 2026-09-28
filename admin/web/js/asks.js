// Pure view model for the host's questions (feat-ops-2): an operation has
// stopped at a step and waits for a person. Each question says what each
// answer sets in motion, and how long the host still waits.

import { humanDuration } from "./format.js";

/**
 * @typedef {{id: number, boot: string | null, op: string, step: string,
 *   what: string, if_allowed: string, if_stopped: string, asked_at: number,
 *   deadline: number}} Ask
 */

/**
 * The questions still open at `now`, oldest first.
 * @param {Ask[]} asks
 * @param {number} now unix seconds
 */
export function openAsks(asks, now) {
  return asks
    .filter((a) => a.deadline > now)
    .sort((a, b) => a.asked_at - b.asked_at || a.id - b.id);
}

/**
 * One question as the page shows it.
 * @param {Ask} a
 * @param {number} now unix seconds
 */
export function askView(a, now) {
  const left = Math.max(0, Math.floor(a.deadline - now));
  return {
    key: `${a.boot ?? ""}:${a.id}`,
    title: `${a.op} is waiting at "${a.step}"`,
    what: a.what,
    ifAllowed: a.if_allowed,
    ifStopped: a.if_stopped,
    left:
      left > 0 ? `${humanDuration(left)} left to answer` : "no longer waiting",
    urgent: left <= 30,
  };
}

/**
 * The body the answer route checks against the questions it heard.
 * @param {Ask} a
 * @param {boolean} allow
 */
export function answerBody(a, allow) {
  return { id: a.id, boot: a.boot, op: a.op, step: a.step, allow };
}
