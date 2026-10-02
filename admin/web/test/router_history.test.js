// fix-210 guard: a page renamed or moved must never strand an old link or
// a Live view script. `fixtures/page_paths_ever_shipped.json` is the
// committed union of every key `router.js`'s PATH_TO_PAGE table has had
// across every tag from v3.63.0 to HEAD (see that file's own comment for
// exactly how it was derived and why the scan starts where it does); this
// test resolves each one through the CURRENT router and fails if a path
// neither resolves to a real page nor is a documented exception.
import { test } from "node:test";
import assert from "node:assert/strict";
import { redirectFor, route } from "../js/router.js";
import FIXTURE from "./fixtures/page_paths_ever_shipped.json" with { type: "json" };

/** @type {Record<string, string>} */
const ALLOWED_404 = FIXTURE.allowed_404;

/**
 * Follow `redirectFor` at most once (every current redirect is one hop;
 * a second hop would be a sign of a new redirect chain this test should
 * be told about explicitly rather than silently following further).
 * @param {string} path
 */
function resolves(path) {
  const r = route(`/${path}`);
  if (r.page !== "notfound") return true;
  return false;
}

test("fix-210: every page path ever shipped still resolves today", () => {
  for (const path of FIXTURE.paths) {
    const allowed = ALLOWED_404[path];
    const ok = resolves(path);
    if (allowed) {
      // A documented exception is pinned to 404 exactly as decided, not
      // merely tolerated — if it starts resolving again this test should
      // be revisited, not silently keep passing either way.
      assert.equal(
        ok,
        false,
        `${path} was expected to still 404 (${allowed}); it now resolves — update the fixture's allowed_404 note`,
      );
      continue;
    }
    assert.equal(
      ok,
      true,
      `/${path} no longer resolves to any page — a page was renamed or removed without an alias (see router.js redirectFor)`,
    );
  }
});

test("fix-210: a redirected historical path lands on a real page, not another redirect", () => {
  for (const path of FIXTURE.paths) {
    if (ALLOWED_404[path]) continue;
    const r = route(`/${path}`);
    const target = redirectFor(r, "");
    if (target == null) continue; // resolves directly, nothing to follow
    const landed = route(target);
    assert.notEqual(
      landed.page,
      "notfound",
      `/${path} redirects to ${target}, which itself does not resolve`,
    );
    assert.equal(
      redirectFor(landed, ""),
      null,
      `/${path} redirects to ${target}, which redirects again — collapse the chain to one hop`,
    );
  }
});
