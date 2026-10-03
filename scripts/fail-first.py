#!/usr/bin/env python3
"""Run the tests a branch added against the code BEFORE it (fix-guards-5).

Kenny, 2026-10-03: "je zou je gedragen als een world-class developer, maar
ik zie vaak amateuristische fouten, die moeten eruit". One of them: a fix
whose new test passed against the old code as well, so it proved nothing
about the fix (fix-133's negative substring assert; fix-171's step counter,
fixed three times; Pause/Stop gone twice, fix-188). "Failed first" was
written in register rows from memory, never checked.

What this does, for BASE..HEAD (BASE defaults to the merge-base with main):

1. Finds the tests the range ADDS that claim a fix, read from HEAD (not the
   working tree): a Rust `#[test]` whose doc block carries `covers: <id>`,
   in a `*/tests/*.rs` file, and a node `test("<id>: …")` in
   `admin/web/test/*.test.js`. A new Rust test WITHOUT `covers:` is listed
   too: nothing claims it, so nothing proves it.
2. Checks BASE out into a throwaway worktree under ~/.cache, copies every
   file the range added or changed under a crate's `tests/` and under
   `admin/web/test/` from HEAD over it (helpers in `common/`, fixtures
   included) — the old code, the new tests — and runs each claimed test by
   name. Cargo builds in a target directory INSIDE that worktree, which is
   deleted with it, so nothing piles up on disk.
3. A verdict per test, from what the runner reported:
     fails on the old code   ≥1 failed — the proof;
     PASSES on the old code  it does not fail without the fix (refused);
     DOES NOT BUILD          against the old code: no proof either way
                             (refused: rewrite it against the old API, or
                             mark `fail-first: <why>` with the constructed
                             case that proves it);
     NOT FOUND               0 tests matched the name (refused).

A test whose guard found nothing in the tree when it was added (a guard
for a future mistake, proven by a constructed bad case instead) carries
`fail-first: <why>` in its doc block and is listed as exempt, with the why.

Tests inside a source file's own `mod tests` cannot be separated from the
code they sit in, so they are listed as not checkable rather than guessed.

Usage: scripts/fail-first.py [BASE]      (make fail-first BASE=<ref>)
Exit 1 when any claimed test is not proven to fail on the old code.
"""
import os
import re
import shutil
import subprocess
import sys
import time

COVERS = re.compile(r"^\s*///?\s*covers:\s*(\S.*)$")
EXEMPT = re.compile(r"^\s*///?\s*fail-first:\s*(\S.*)$")
NODE_TEST = re.compile(r"""^\+\s*test\(\s*(["'`])((?:[a-z]+-)+\d+:[^"'`]*)\1""")
TESTS_TREE = re.compile(r"^(?:[^/]+/tests/.+|admin/web/test/.+)$")


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout


def rust_tests(path_text):
    """(name, covers?, exempt reason or None) for every #[test] fn."""
    lines = path_text.splitlines()
    out = []
    for i, line in enumerate(lines):
        m = re.match(r"\s*(?:pub\s+)?(?:async\s+)?fn\s+([a-z0-9_]+)\s*\(", line)
        if not m:
            continue
        j, is_test, covers, exempt = i - 1, False, False, None
        while j >= 0 and (lines[j].strip().startswith(("#[", "//")) or not lines[j].strip()):
            s = lines[j]
            if s.strip().startswith(("#[test", "#[tokio::test")):
                is_test = True
            if COVERS.match(s):
                covers = True
            e = EXEMPT.match(s)
            if e:
                exempt = e.group(1)
            if not s.strip():
                break
            j -= 1
        if is_test:
            out.append((m.group(1), covers, exempt))
    return out


def changed_files(base):
    return git("diff", "--name-only", "--diff-filter=AM", f"{base}..HEAD").split()


def added_tests(base):
    """({file: [(kind, name, exempt)]}, [unclaimed "file :: name"]) for the
    tests BASE..HEAD adds, read from HEAD."""
    found, unclaimed = {}, []
    for f in changed_files(base):
        if re.match(r"[^/]+/tests/[^/]+\.rs$", f):
            new = rust_tests(git("show", f"HEAD:{f}"))
            try:
                old_text = git("show", f"{base}:{f}")
            except subprocess.CalledProcessError:
                old_text = ""
            old = {n for n, _, _ in rust_tests(old_text)}
            claims = [("rust", n, e) for n, c, e in new if n not in old and c]
            unclaimed += [f"{f} :: {n}" for n, c, _ in new if n not in old and not c]
            if claims:
                found[f] = claims
        elif re.match(r"admin/web/test/[^/]+\.test\.js$", f):
            diff = git("diff", "-U0", f"{base}..HEAD", "--", f)
            claims = [("node", m.group(2), None) for m in map(NODE_TEST.match, diff.splitlines()) if m]
            if claims:
                found[f] = claims
    return found, unclaimed


def in_source_tests(base):
    """Claimed tests added inside src/ files (not separable)."""
    out = []
    for f in changed_files(base):
        if f.endswith(".rs") and "/src/" in f:
            diff = git("diff", "-U0", f"{base}..HEAD", "--", f)
            if re.search(r"^\+\s*///?\s*covers:", diff, re.M):
                out.append(f)
    return out


def package_of(path):
    toml = git("show", f"HEAD:{path.split('/')[0]}/Cargo.toml")
    return re.search(r'^name\s*=\s*"([^"]+)"', toml, re.M).group(1)


def rust_verdict(out):
    """The verdict from cargo's own report, never from its exit code alone."""
    if "error[E" in out or "could not compile" in out:
        return "DOES NOT BUILD against the old code"
    ran = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", out)
    if not ran:
        last = next((l for l in reversed(out.splitlines()) if l.strip()), "no output")
        return f"COULD NOT RUN ({last.strip()[:80]})"
    if int(ran.group(1)) + int(ran.group(2)) == 0:
        return "NOT FOUND (0 tests matched)"
    if int(ran.group(2)) >= 1:
        return "fails on the old code"
    return "PASSES on the old code"


def node_verdict(out):
    text = out.replace("ℹ", "#")
    fail = re.search(r"^# fail (\d+)", text, re.M)
    passed = re.search(r"^# pass (\d+)", text, re.M)
    if fail and int(fail.group(1)) >= 1:
        return "fails on the old code"
    if passed and int(passed.group(1)) >= 1:
        return "PASSES on the old code"
    return "NOT FOUND (0 tests matched)"


def main(argv):
    root = git("rev-parse", "--show-toplevel").strip()
    os.chdir(root)
    base = argv[1] if len(argv) > 1 else git("merge-base", "HEAD", "main").strip()
    base_sha = git("rev-parse", "--short", base).strip()
    found, unclaimed = added_tests(base)
    for f in in_source_tests(base):
        print(f"not checkable (a test module inside a source file): {f}")
    for u in unclaimed:
        print(f"{'no covers: (nothing claims it)':<46} {u}")
    if not found:
        print(f"fail-first: no claimed tests added since {base_sha}")
        return 0
    work = os.path.join(os.environ.get("FAIL_FIRST_DIR") or os.path.expanduser("~/.cache/homelab-fail-first"), base_sha)
    if os.path.exists(work):
        subprocess.run(["git", "worktree", "remove", "--force", work], capture_output=True)
        shutil.rmtree(work, ignore_errors=True)
    os.makedirs(os.path.dirname(work), exist_ok=True)
    git("worktree", "add", "--detach", work, base)
    env = dict(os.environ, CARGO_TARGET_DIR=os.path.join(work, ".fail-first-target"))
    refused, results = [], []
    try:
        # Every test-side file the range touched, from HEAD: the claimed
        # tests' own files and the helpers and fixtures they read.
        for f in changed_files(base):
            if TESTS_TREE.match(f):
                dest = os.path.join(work, f)
                os.makedirs(os.path.dirname(dest), exist_ok=True)
                with open(dest, "w", encoding="utf-8") as out:
                    out.write(git("show", f"HEAD:{f}"))
        if any(k == "node" for c in found.values() for k, _, _ in c):
            nm = os.path.join(root, "admin/web/node_modules")
            if os.path.isdir(nm) and not os.path.exists(os.path.join(work, "admin/web/node_modules")):
                os.symlink(nm, os.path.join(work, "admin/web/node_modules"))
        for f, claims in found.items():
            for kind, name, exempt in claims:
                if exempt:
                    results.append(("exempt", f, name, exempt))
                    continue
                start = time.time()
                if kind == "rust":
                    stem = os.path.splitext(os.path.basename(f))[0]
                    cmd = ["cargo", "test", "-q", "-p", package_of(f), "--test", stem, "--", "--exact", name]
                    r = subprocess.run(cmd, cwd=work, capture_output=True, text=True,
                                       stdin=subprocess.DEVNULL, env=env)
                    verdict = rust_verdict(r.stdout + r.stderr)
                else:
                    cmd = ["node", "--test", "--test-name-pattern", "^" + re.escape(name) + "$",
                           os.path.relpath(f, "admin/web")]
                    r = subprocess.run(cmd, cwd=os.path.join(work, "admin/web"), capture_output=True, text=True)
                    verdict = node_verdict(r.stdout + r.stderr)
                took = time.time() - start
                results.append((verdict, f, name, f"{int(took // 60)} min {took % 60:.1f} s"))
                if not verdict.startswith("fails"):
                    refused.append(name)
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", work], capture_output=True)
        shutil.rmtree(work, ignore_errors=True)
    for verdict, f, name, note in results:
        print(f"{verdict:<46} {f} :: {name}  ({note})")
    if refused:
        print(f"\nFAIL-FIRST REFUSED — {len(refused)} claimed test(s) are not proven to fail against "
              f"{base_sha}, the code before the fix (passing, not building, or not found).",
              file=sys.stderr)
        return 1
    print(f"\nfail-first: every claimed test fails against {base_sha}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
