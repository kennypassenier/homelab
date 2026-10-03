// Pure view models for the activity page (feat-ops-1, feat-ops-7): the
// incident bundles and the host's history, as rows a person reads.

import { humanDuration } from "./format.js";

/**
 * @typedef {{step: string, start: number, end: number, changed: boolean}} Step
 * @typedef {{kind: "op", start: number, end: number, label: string,
 *   subject?: string | null, req?: number | null, by?: string | null, ok: boolean,
 *   deferred?: string | null, error?: string | null, steps: Step[]}} OpEntry
 * @typedef {{kind: "phase", start: number, end: number, name: string,
 *   count: number}} PhaseEntry
 * @typedef {OpEntry | PhaseEntry} Entry
 * @typedef {{label: string, tone: "ok" | "warn" | "bad"}} Outcome
 */

/**
 * An incident bundle's name is `<unix seconds>-<operation>`, for example
 * "1790000000-deploy-media".
 * @param {string} name
 * @returns {{at: number | null, op: string}}
 */
export function parseIncident(name) {
  const m = /^(\d+)-(.+)$/.exec(name);
  if (!m) return { at: null, op: name };
  return { at: Number(m[1]), op: m[2] };
}

/**
 * The incident table's rows, newest first.
 * @param {string[]} names
 */
export function incidentRows(names) {
  return names
    .map((name) => ({ name, ...parseIncident(name) }))
    .sort((a, b) => (b.at ?? 0) - (a.at ?? 0));
}

/**
 * What an entry was, in a few words.
 * @param {Entry} e
 */
export function entryWhat(e) {
  if (e.kind === "phase")
    return `nightly ${e.name} (${e.count} ${e.count === 1 ? "job" : "jobs"})`;
  return e.subject || e.label;
}

/**
 * How it ended. A deferred operation is a decision, not a fault (F280).
 * @param {Entry} e
 * @returns {Outcome}
 */
export function entryOutcome(e) {
  if (e.kind === "phase") return { label: "done", tone: "ok" };
  if (e.deferred) return { label: "deferred", tone: "warn" };
  if (e.end === 0 || e.end < e.start) return { label: "running", tone: "warn" };
  return e.ok ? { label: "ok", tone: "ok" } : { label: "failed", tone: "bad" };
}

/**
 * redesign-3.71 secrets: a reveal or a copy of a secret, as the host
 * records it (label `reveal-secret` / `copy-secret`, subject "revealed
 * gateway/traefik/.env", `by` the person or "Claude (Live view)").
 * @param {Entry} e
 */
export const isSecretAudit = (e) =>
  e.kind === "op" && (e.label === "reveal-secret" || e.label === "copy-secret");

/**
 * The history table's rows, newest first.
 * @param {Entry[]} entries
 */
export function historyRows(entries) {
  return entries
    .filter((e) => e && (e.kind === "op" || e.kind === "phase"))
    .map((e) => {
      if (e.kind === "op" && isSecretAudit(e)) {
        // "Kenny revealed gateway/traefik/.env": who, what, never the value.
        const who = e.by ?? (e.req != null ? "asked" : "host");
        return {
          start: e.start,
          what: `${who} ${e.subject ?? e.label}`,
          took: null,
          duration: "—",
          outcome: entryOutcome(e),
          by: who,
          detail: e.error ?? "the value itself is never recorded",
        };
      }
      const took = e.end >= e.start && e.end > 0 ? e.end - e.start : null;
      const detail =
        e.kind === "op"
          ? (e.deferred ??
            e.error ??
            `${e.steps.length} ${e.steps.length === 1 ? "step" : "steps"}, ${e.steps.filter((s) => s.changed).length} changed`)
          : "";
      return {
        start: e.start,
        what: entryWhat(e),
        took,
        duration: took == null ? "—" : humanDuration(took),
        outcome: entryOutcome(e),
        by:
          e.kind === "phase"
            ? "nightly round"
            : e.req != null
              ? "asked"
              : "host",
        detail,
      };
    })
    .sort((a, b) => b.start - a.start);
}
