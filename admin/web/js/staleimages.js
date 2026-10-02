// fix-231 / fix-232: the Fleet view's stale-image rows, as plain data —
// whether a move is a MAJOR version jump (the update dialog then asks for
// the release notes to be read first), and the absolute link to the newer
// version's release page (Kenny, 2026-10-02: "en de links werken niet" —
// an upstream shown as `github.com/owner/repo` text read as a link and
// went nowhere). No DOM here, so node tests hold both.

/**
 * The numbers in a version, in order: `v10.11.11` → [10, 11, 11],
 * linuxserver's `5.2.3_v2.0.14-ls474` → [5, 2, 3, 2, 0, 14, 474]. The
 * same reading the host's own pin check uses (`core::ops::pins::numbers`).
 * @param {string} v
 * @returns {number[]}
 */
export function versionNumbers(v) {
  return (String(v).match(/\d+/g) ?? []).map(Number);
}

/**
 * A MAJOR version jump: the first number differs (`10.11.11` → `v12.1`).
 * A version with no number at all is never called one — nothing to read.
 * @param {string} from
 * @param {string} to
 */
export function majorJump(from, to) {
  const a = versionNumbers(from);
  const b = versionNumbers(to);
  if (!a.length || !b.length) return false;
  return a[0] !== b[0];
}

/**
 * `owner/repo` of an upstream as the fleet check names it
 * (`github.com/owner/repo`, possibly with a scheme, `www.` or `.git`);
 * null for anything that is not a GitHub repository.
 * @param {string} upstream
 * @returns {{owner: string, repo: string} | null}
 */
export function githubRepo(upstream) {
  const m =
    /^(?:https?:\/\/)?(?:www\.)?github\.com\/([A-Za-z0-9_.-]+)\/([A-Za-z0-9_.-]+?)(?:\.git)?\/?$/.exec(
      String(upstream ?? "").trim(),
    );
  if (!m || m[1] === "." || m[1] === ".." || m[2] === "." || m[2] === "..")
    return null;
  return { owner: m[1], repo: m[2] };
}

/**
 * The newer version's release page, absolute:
 * `https://github.com/<owner>/<repo>/releases/tag/<latest>`. Null when the
 * upstream is not a GitHub repository or there is no tag to point at.
 * @param {string} upstream
 * @param {string} latest
 * @returns {string | null}
 */
export function releaseUrl(upstream, latest) {
  const r = githubRepo(upstream);
  const tag = String(latest ?? "").trim();
  if (!r || !tag) return null;
  return `https://github.com/${r.owner}/${r.repo}/releases/tag/${encodeURIComponent(tag)}`;
}
