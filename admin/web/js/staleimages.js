// fix-231 / fix-232: the Fleet view's stale-image rows, as plain data —
// whether a move is a MAJOR version jump (the update dialog then asks for
// the release notes to be read first), and the absolute link to the newer
// version's release page. The reading of versions and of GitHub upstreams is
// kp's version-delta module (the same one the chips use); this file only
// names the two questions the pages ask. No DOM here, so node tests hold both.

import {
  githubRepo,
  releaseUrl,
  versionLevel,
  versionNumbers,
} from "/static/kp/dist/kp-themes.min.js";

export { githubRepo, releaseUrl, versionNumbers };

/**
 * A MAJOR version jump: the first number differs (`10.11.11` → `v12.1`).
 * A version with no number at all is never called one — nothing to read.
 * @param {string} from
 * @param {string} to
 */
export const majorJump = (from, to) => versionLevel(from, to) === "major";
