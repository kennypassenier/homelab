// fix-guards-8: text a person reads in the dashboard never carries a bare
// register id ("fix-240", "feat-shell-1", "ask-9") or an ISO time.
//
// Kenny, 2026-10-03: "je zou je gedragen als een world-class developer,
// maar ik zie vaak amateuristische fouten, die moeten eruit". A register id
// means something in this repository's REGISTER.md and nothing to the
// person reading a page; the Retired page's description said "(ask-9)" and
// three host.toml descriptions on Settings named fix-170, fix-184 and
// gap-26. Ids belong in comments and commits, where they link code to its
// reason; a page says what the thing does.
//
// What is scanned: every string literal and template-literal text in
// admin/web/js (comments skipped by a small lexer), the text of
// admin/web/index.html, and every string in formspec.json except its
// "about" note. A string with no whitespace is a key or an identifier, not
// prose, and is not judged; an id inside [brackets] is a commit trailer,
// written for git, and is allowed.

import { test } from "node:test";
import assert from "node:assert/strict";
import { readFileSync, readdirSync, statSync } from "node:fs";
import { join, dirname, relative } from "node:path";
import { fileURLToPath } from "node:url";

const web = join(dirname(fileURLToPath(import.meta.url)), "..");

const ID =
  /(?<![\w-])(?:fix|feat|gap|redesign|arch|step|scope|ask|tech)-(?:[a-z]+-)*\d+(?![\w.-]*\d)(?![\w-])/;
const ISO = /\d{4}-\d{2}-\d{2}T\d{2}:\d{2}/;

/**
 * Every string literal and template text in `src`, comments skipped.
 * @param {string} src
 * @returns {string[]}
 */
export function stringsOf(src) {
  /** @type {string[]} */
  const out = [];
  let i = 0;
  let prev = ""; // last significant character outside strings/comments
  let prevWord = "";
  const regexAfter = "(,=:[!&|?{};+-*%<>~^";
  const n = src.length;
  /**
   * Read a quoted string starting at `at` (the quote); returns its end.
   * @param {string} q
   * @param {number} at
   * @returns {number}
   */
  const quoted = (q, at) => {
    let j = at + 1;
    let s = "";
    while (j < n && src[j] !== q) {
      if (src[j] === "\\") {
        s += src[j + 1] ?? "";
        j += 2;
        continue;
      }
      if (src[j] === "\n") break;
      s += src[j++];
    }
    out.push(s);
    return j + 1;
  };
  /**
   * Read a template literal starting at `at` (the backtick).
   * @param {number} at
   * @returns {number}
   */
  const template = (at) => {
    let j = at + 1;
    let s = "";
    while (j < n && src[j] !== "`") {
      if (src[j] === "\\") {
        s += src[j + 1] ?? "";
        j += 2;
        continue;
      }
      if (src[j] === "$" && src[j + 1] === "{") {
        s += " ";
        j = skipExpr(j + 2);
        continue;
      }
      s += src[j++];
    }
    out.push(s);
    return j + 1;
  };
  /**
   * Skip a ${…} expression (strings inside it are collected too).
   * @param {number} at
   * @returns {number}
   */
  const skipExpr = (at) => {
    let depth = 1;
    let j = at;
    while (j < n && depth > 0) {
      const c = src[j];
      if (c === "'" || c === '"') j = quoted(c, j);
      else if (c === "`") j = template(j);
      else {
        if (c === "{") depth++;
        if (c === "}") depth--;
        j++;
      }
    }
    return j;
  };
  while (i < n) {
    const c = src[i];
    if (c === "/" && src[i + 1] === "/") {
      while (i < n && src[i] !== "\n") i++;
      continue;
    }
    if (c === "/" && src[i + 1] === "*") {
      const end = src.indexOf("*/", i + 2);
      i = end < 0 ? n : end + 2;
      continue;
    }
    if (c === "'" || c === '"') {
      i = quoted(c, i);
      prev = "a";
      continue;
    }
    if (c === "`") {
      i = template(i);
      prev = "a";
      continue;
    }
    if (
      c === "/" &&
      (prev === "" ||
        regexAfter.includes(prev) ||
        ["return", "typeof", "case"].includes(prevWord))
    ) {
      let j = i + 1;
      let inClass = false;
      while (j < n && src[j] !== "\n") {
        if (src[j] === "\\") {
          j += 2;
          continue;
        }
        if (src[j] === "[") inClass = true;
        else if (src[j] === "]") inClass = false;
        else if (src[j] === "/" && !inClass) break;
        j++;
      }
      i = j + 1;
      prev = "a";
      continue;
    }
    if (/\w/.test(c)) {
      let j = i;
      while (j < n && /[\w$]/.test(src[j])) j++;
      prevWord = src.slice(i, j);
      prev = "a";
      i = j;
      continue;
    }
    if (!/\s/.test(c)) {
      prev = c;
      prevWord = "";
    }
    i++;
  }
  return out;
}

/**
 * Why `text` may not reach a person, or null.
 * @param {string} text
 * @returns {string | null}
 */
export function userTextFault(text) {
  if (!/\s/.test(text.trim())) return null; // a key or identifier, not prose
  const withoutTrailers = text.replace(/\[[^\]]*\]/g, "");
  const id = withoutTrailers.match(ID);
  if (id) return `names the register id "${id[0]}"`;
  if (ISO.test(text)) return "shows an ISO time";
  return null;
}

/**
 * @param {string} dir
 * @param {string} ext
 * @param {string[]} [out]
 * @returns {string[]}
 */
function walk(dir, ext, out = []) {
  for (const name of readdirSync(dir)) {
    if (name === "node_modules" || name === "test" || name === "test-e2e")
      continue;
    const p = join(dir, name);
    if (statSync(p).isDirectory()) walk(p, ext, out);
    else if (p.endsWith(ext)) out.push(p);
  }
  return out;
}

/** All faults in the dashboard as it is on disk. */
function faults() {
  /** @type {string[]} */
  const found = [];
  for (const file of walk(join(web, "js"), ".js")) {
    for (const s of stringsOf(readFileSync(file, "utf8"))) {
      const why = userTextFault(s);
      if (why)
        found.push(
          `${relative(web, file)}: ${why}: ${JSON.stringify(s.slice(0, 120))}`,
        );
    }
  }
  const html = readFileSync(join(web, "index.html"), "utf8").replace(
    /<!--[\s\S]*?-->/g,
    "",
  );
  for (const s of html.split(/<[^>]*>/)) {
    const why = userTextFault(s);
    if (why)
      found.push(
        `index.html: ${why}: ${JSON.stringify(s.trim().slice(0, 120))}`,
      );
  }
  const spec = JSON.parse(
    readFileSync(join(web, "js", "formspec.json"), "utf8"),
  );
  /**
   * @param {unknown} v
   * @param {string} key
   */
  const visit = (v, key) => {
    if (typeof v === "string") {
      const why = key === "about" ? null : userTextFault(v);
      if (why)
        found.push(
          `formspec.json ${key}: ${why}: ${JSON.stringify(v.slice(0, 120))}`,
        );
    } else if (Array.isArray(v)) v.forEach((x) => visit(x, key));
    else if (v && typeof v === "object")
      for (const [k, x] of Object.entries(v)) visit(x, k);
  };
  visit(spec, "");
  return found;
}

test("fix-guards-8: no dashboard text a person reads names a register id or an ISO time", () => {
  const found = faults();
  assert.deepEqual(found, [], `say what it does instead:\n${found.join("\n")}`);
});

test("the user-text lexer finds strings, skips comments, and judges only prose", () => {
  const src = [
    "// a comment naming fix-240 is fine",
    "/* so is feat-shell-1 here */",
    'const a = "Every stack a destroy retired (ask-9) is kept";',
    "const b = `Restart ${name} now, fix-129 says so`;",
    "const c = x / 2 / y; const r = /fix-1/;",
    'const d = "feat-stacks-3"; const e = "chore: x [fix-110]";',
    'const f = "redesign-3.71/backups.html";',
    'const g = "taken 2026-10-03T12:36 local";',
  ].join("\n");
  const strings = stringsOf(src);
  assert.ok(!strings.some((s) => s.includes("a comment")));
  const judged = strings.map((s) => [s, userTextFault(s)]).filter(([, w]) => w);
  assert.deepEqual(
    judged.map(([s]) => s),
    [
      "Every stack a destroy retired (ask-9) is kept",
      "Restart   now, fix-129 says so",
      "taken 2026-10-03T12:36 local",
    ],
  );
});
