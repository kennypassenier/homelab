// fix-231 / fix-232: the stale-image rows' two pure decisions — is a move a
// MAJOR version jump (the update dialog then requires the release notes to
// be ticked as read), and where does the source link go (an absolute https
// link to the newer version's release page, never `github.com/...` text).
import { test } from "node:test";
import assert from "node:assert/strict";
import {
  githubRepo,
  majorJump,
  releaseUrl,
  versionNumbers,
} from "../js/staleimages.js";

test("fix-231: a different first number is a major jump, whatever the decoration", () => {
  assert.equal(majorJump("10.11.11", "v12.1"), true);
  assert.equal(majorJump("v3.7.12", "v4.0.0"), true);
  assert.equal(majorJump("1.4.2", "2.0.0"), true);
});

test("fix-231: the same first number is not a major jump", () => {
  assert.equal(majorJump("10.11.11", "10.11.12"), false);
  assert.equal(majorJump("v2.3.0", "v2.4.0"), false);
  assert.equal(majorJump("5.2.3_v2.0.14-ls474", "5.2.3_v2.0.14-ls475"), false);
  assert.equal(majorJump("2026.8.3", "2026.9.0"), false);
});

test("fix-231: a version without numbers is never called a major jump", () => {
  assert.equal(majorJump("latest", "v2.0.0"), false);
  assert.equal(majorJump("1.0", "nightly"), false);
  assert.deepEqual(
    versionNumbers("5.2.3_v2.0.14-ls474"),
    [5, 2, 3, 2, 0, 14, 474],
  );
});

test("fix-232: the release link is absolute https to the newer tag's release page", () => {
  assert.equal(
    releaseUrl("github.com/jellyfin/jellyfin", "v10.11.12"),
    "https://github.com/jellyfin/jellyfin/releases/tag/v10.11.12",
  );
  assert.equal(
    releaseUrl("https://github.com/Tecnativa/docker-socket-proxy.git", "0.4.0"),
    "https://github.com/Tecnativa/docker-socket-proxy/releases/tag/0.4.0",
  );
  // A tag with characters a URL path cannot carry as they are.
  assert.equal(
    releaseUrl("github.com/o/r", "5.2.3_v2.0.14+ls475"),
    "https://github.com/o/r/releases/tag/5.2.3_v2.0.14%2Bls475",
  );
});

test("fix-232: no link for an upstream that is not a GitHub repository, or no tag", () => {
  assert.equal(releaseUrl("gitlab.com/o/r", "1.0"), null);
  assert.equal(releaseUrl("github.com/only-owner", "1.0"), null);
  assert.equal(releaseUrl("github.com/o/r", ""), null);
  assert.equal(githubRepo("github.com/../x"), null);
  assert.deepEqual(githubRepo("www.github.com/o/r/"), {
    owner: "o",
    repo: "r",
  });
});
