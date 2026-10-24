// checks-notice-link-1: the fleet check's latest round as the Inbox's findings
// view reads it (checkfindings.js draws it). Pure: no DOM, no fetch.

import { checkRefOf } from "./editforms.js";
import { findingRows } from "./parity.js";

/** The Inbox section this view is, in `?section=`. */
export const FINDINGS_SECTION = "check-findings";

/**
 * The latest round as the view draws it: Broken first, then Drift, the
 * noted ones apart, each finding with the check it is about when its line
 * names one.
 * @param {{findings?: any[], passes?: number} | null | undefined} body
 */
export function findingsView(body) {
  const rows = findingRows(body?.findings ?? []).map((r) => ({
    ...r,
    ref: checkRefOf(`${r.subject}: ${r.what}`),
  }));
  const broken = rows.filter((r) => r.badge.label === "broken");
  const drift = rows.filter((r) => r.badge.label === "drift");
  const noted = rows.filter(
    (r) => r.badge.label !== "broken" && r.badge.label !== "drift",
  );
  return {
    broken,
    drift,
    noted,
    passes: Number(body?.passes) || 0,
    summary: `${broken.length} broken · ${drift.length} drift${
      noted.length ? ` · ${noted.length} noted` : ""
    }`,
  };
}
