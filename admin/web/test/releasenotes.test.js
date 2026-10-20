import assert from "node:assert/strict";
import { test } from "node:test";
import { notesQuery } from "../js/updateflow.js";

test("only a GitHub release link names notes to fetch", () => {
  assert.equal(
    notesQuery("https://github.com/immich-app/immich/releases/tag/v2.0.0"),
    "repo=immich-app%2Fimmich&tag=v2.0.0",
  );
  assert.equal(notesQuery("https://example.com/o/r/releases/tag/v1"), null);
  assert.equal(notesQuery("https://github.com/o/r/releases"), null);
  assert.equal(notesQuery(null), null);
});
