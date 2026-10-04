// fix-216 (Kenny, 2026-10-02: "ik heb toch geen idee wat die snapshot is?
// Ik dacht dat je de snapshots ging tonen, met hun datum etc erbij?"): the
// restore dialog's snapshot field becomes a picker — every snapshot of the
// app or unit a row already chose, newest first, dated and aged, the
// newest preselected and labelled "latest". Pure, so the shape a browser
// renders is the same shape a node test can pin without a DOM or a fetch;
// `actiondialog.js` only draws what this returns.

import { formatDateTime } from "./format.js";

/**
 * @typedef {{id: string, short_id: string, time: number,
 *   run?: number | null, size_bytes?: number | null,
 *   file_count?: number | null, trigger?: string | null}} SnapRun
 * @typedef {{value: string, shortId: string, when: string, ago: string,
 *   latest: boolean, selected: boolean, size: string | null,
 *   files: string | null, kind: string | null}} SnapshotRow
 */

/**
 * One row's "N days ago" (or hours/minutes for a recent one), from the
 * same unit ladder `humanDuration` gives durations in.
 * @param {number} seconds
 */
function agoWords(seconds) {
  const s = Math.max(0, Math.round(seconds));
  const m = Math.floor(s / 60);
  const h = Math.floor(s / 3600);
  const d = Math.floor(s / 86400);
  if (d >= 1) return `${d} ${d === 1 ? "day" : "days"} ago`;
  if (h >= 1) return `${h} ${h === 1 ? "hour" : "hours"} ago`;
  if (m >= 1) return `${m} ${m === 1 ? "minute" : "minutes"} ago`;
  return "just now";
}

/**
 * A size in bytes as a short human string, or null when not given (the
 * host does not report a per-snapshot size today — fix-216's report).
 * @param {number | null | undefined} bytes
 */
function humanSize(bytes) {
  if (bytes == null || !Number.isFinite(bytes) || bytes < 0) return null;
  if (bytes < 1024) return `${bytes} B`;
  const units = ["KB", "MB", "GB", "TB"];
  let v = bytes / 1024;
  let i = 0;
  while (v >= 1024 && i < units.length - 1) {
    v /= 1024;
    i += 1;
  }
  return `${v < 10 ? v.toFixed(1) : Math.round(v)} ${units[i]}`;
}

/**
 * The rows a snapshot picker draws, newest first, the newest always
 * `latest: true` regardless of input order (defensively re-sorted — the
 * host already answers newest-first, `core::ops::backup::RepoStatus`'s own
 * doc comment, but a picker that silently trusted that and got it wrong
 * would mislabel "latest").
 *
 * `selected` follows the field's existing contract: an empty string (or a
 * value matching the newest snapshot) means "latest" and is pre-checked on
 * the first row; any other id is pre-checked on the row it matches.
 * @param {SnapRun[]} snapshots
 * @param {number} now unix seconds
 * @param {string} [selected]
 * @returns {SnapshotRow[]}
 */
export function snapshotPickerRows(snapshots, now, selected = "") {
  const sorted = [...snapshots].sort((a, b) => b.time - a.time);
  return sorted.map((s, i) => {
    const latest = i === 0;
    // The latest row's underlying value is "" (empty = latest, the field's
    // own long-standing contract) so a fresh dialog — no explicit choice
    // made yet — keeps sending exactly what it always sent.
    const value = latest ? "" : s.short_id;
    const isSelected = latest
      ? selected === "" || selected === s.short_id || selected === s.id
      : selected === s.short_id || selected === s.id;
    return {
      value,
      shortId: s.short_id,
      when: formatDateTimeBrussels(s.time),
      ago: agoWords(Math.max(0, now - s.time)),
      latest,
      selected: isSelected,
      size: humanSize(s.size_bytes ?? null),
      files:
        s.file_count != null && Number.isFinite(s.file_count)
          ? `${s.file_count} ${s.file_count === 1 ? "file" : "files"}`
          : null,
      // fix-223: what made this backup (nightly, manual, pre-destroy), from
      // the host's own trigger tag; null for a snapshot taken before the
      // tag existed — never guessed.
      kind: typeof s.trigger === "string" && s.trigger ? s.trigger : null,
    };
  });
}

/**
 * A snapshot's moment in Europe/Brussels — fixed, not the viewer's own zone
 * (Kenny, 2026-10-02: a restore's "when" must read the same on every
 * screen it is driven from), in the one date format (redesign-final X4).
 * @param {number} unixSeconds
 */
export function formatDateTimeBrussels(unixSeconds) {
  return formatDateTime(unixSeconds, { timeZone: "Europe/Brussels" });
}

/**
 * Which repository (owner) a snapshot picker reads: the locked/chosen
 * `app` value when the form has one and it matches a repository, else the
 * one repository a single-owner stack (most native services) has, else
 * none — ambiguous, so the picker says so instead of guessing.
 * @param {{owner: string, snapshots?: SnapRun[]}[]} repos
 * @param {string} [appValue]
 * @returns {{owner: string, snapshots?: SnapRun[]} | null}
 */
export function resolveSnapshotOwner(repos, appValue) {
  if (appValue) {
    const match = repos.find((r) => r.owner === appValue);
    if (match) return match;
  }
  return repos.length === 1 ? repos[0] : null;
}
