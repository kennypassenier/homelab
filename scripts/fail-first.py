#!/usr/bin/env python3
"""Run the tests a branch added against the code BEFORE it (fix-guards-5).

Kenny, 2026-10-03: "je zou je gedragen als een world-class developer, maar
ik zie vaak amateuristische fouten, die moeten eruit". One of them: a fix
whose new test passed against the old code as well, so it proved nothing
about the fix (fix-133's negative substring assert; fix-171's step counter,
fixed three times; Pause/Stop gone twice, fix-188). "Failed first" was
written in register rows from memory, never checked.

What this does, for BASE..HEAD (BASE defaults to the merge-base with main):

1. Finds the tests the range ADDS that claim a fix: a Rust `#[test]` whose
   doc block carries `covers: <id>`, in a `*/tests/*.rs` file, and a node
   `test("<id>: …")` in `admin/web/test/*.test.js`.
2. Checks BASE out into a throwaway worktree under ~/.cache, copies only
   those test FILES from HEAD over it — the old code, the new tests — and
   runs each claimed test by name.
3. A test that PASSES there is reported: it does not fail without the fix.
   A test that does not even build against the old code counts as failing
   (it needs the new code), and says so.

A test whose guard found nothing in the tree when it was added (a guard
for a future mistake, proven by a constructed bad case instead) carries
`fail-first: <why>` in its doc block and is listed as exempt, with the why.

Tests inside a source file's own `mod tests` cannot be separated from the
code they sit in, so they are listed as not checkable rather than guessed.

Usage: scripts/fail-first.py [BASE]      (make fail-first BASE=<ref>)
Exit 1 when any claimed test passes on the old code.
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


def git(*args, cwd=None):
    return subprocess.run(["git", *args], cwd=cwd, capture_output=True, text=True, check=True).stdout


def rust_claims(path_text):
    """(name, exempt reason or None) for every covers:-marked test fn."""
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
        if is_test and covers:
            out.append((m.group(1), exempt))
    return out


def added_tests(base):
    """{file: [(kind, name, exempt)]} for the claimed tests BASE..HEAD adds."""
    files = git("diff", "--name-only", "--diff-filter=AM", f"{base}..HEAD").split()
    found = {}
    for f in files:
        if re.match(r"[^/]+/tests/[^/]+\.rs$", f):
            new = rust_claims(open(f, encoding="utf-8").read())
            try:
                old_text = git("show", f"{base}:{f}")
            except subprocess.CalledProcessError:
                old_text = ""
            old = {n for n, _ in rust_claims(old_text)}
            claims = [("rust", n, e) for n, e in new if n not in old]
            if claims:
                found[f] = claims
        elif re.match(r"admin/web/test/[^/]+\.test\.js$", f):
            diff = git("diff", "-U0", f"{base}..HEAD", "--", f)
            claims = [("node", m.group(2), None) for m in map(NODE_TEST.match, diff.splitlines()) if m]
            if claims:
                found[f] = claims
    return found


def in_source_tests(base):
    """Claimed tests added inside src/ files (not separable)."""
    out = []
    for f in git("diff", "--name-only", "--diff-filter=AM", f"{base}..HEAD").split():
        if f.endswith(".rs") and "/src/" in f:
            diff = git("diff", "-U0", f"{base}..HEAD", "--", f)
            if re.search(r"^\+\s*///?\s*covers:", diff, re.M):
                out.append(f)
    return out


def package_of(path):
    toml = open(os.path.join(path.split("/")[0], "Cargo.toml"), encoding="utf-8").read()
    return re.search(r'^name\s*=\s*"([^"]+)"', toml, re.M).group(1)


def main(argv):
    root = git("rev-parse", "--show-toplevel").strip()
    os.chdir(root)
    base = argv[1] if len(argv) > 1 else git("merge-base", "HEAD", "main").strip()
    base_sha = git("rev-parse", "--short", base).strip()
    found = added_tests(base)
    for f in in_source_tests(base):
        print(f"not checkable (a test module inside a source file): {f}")
    if not found:
        print(f"fail-first: no claimed tests added since {base_sha}")
        return 0
    work = os.path.join(os.environ.get("FAIL_FIRST_DIR") or os.path.expanduser("~/.cache/homelab-fail-first"), base_sha)
    if os.path.exists(work):
        subprocess.run(["git", "worktree", "remove", "--force", work], capture_output=True)
        shutil.rmtree(work, ignore_errors=True)
    os.makedirs(os.path.dirname(work), exist_ok=True)
    git("worktree", "add", "--detach", work, base)
    passed_on_old, results = [], []
    try:
        for f in found:
            shutil.copyfile(f, os.path.join(work, f))
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
                    r = subprocess.run(cmd, cwd=work, capture_output=True, text=True, stdin=subprocess.DEVNULL)
                    out = r.stdout + r.stderr
                    built = "error[E" not in out and "could not compile" not in out
                    ran = re.search(r"test result: \w+\. (\d+) passed; (\d+) failed", out)
                    if r.returncode == 0 and ran and ran.group(1) == "1":
                        verdict = "PASSES on the old code"
                    elif not built:
                        verdict = "fails (does not build against the old code)"
                    else:
                        verdict = "fails"
                else:
                    cmd = ["node", "--test", "--test-name-pattern", "^" + re.escape(name) + "$",
                           os.path.relpath(f, "admin/web")]
                    r = subprocess.run(cmd, cwd=os.path.join(work, "admin/web"), capture_output=True, text=True)
                    out = r.stdout + r.stderr
                    verdict = "PASSES on the old code" if r.returncode == 0 and "# pass 1" in out.replace("ℹ", "#") else "fails"
                took = time.time() - start
                results.append((verdict, f, name, f"{int(took // 60)} min {took % 60:.1f} s"))
                if verdict.startswith("PASSES"):
                    passed_on_old.append(name)
    finally:
        subprocess.run(["git", "worktree", "remove", "--force", work], capture_output=True)
    for verdict, f, name, note in results:
        print(f"{verdict:<46} {f} :: {name}  ({note})")
    if passed_on_old:
        print(f"\nFAIL-FIRST REFUSED — {len(passed_on_old)} test(s) pass against {base_sha}, "
              "the code before the fix, so they prove nothing about it.", file=sys.stderr)
        return 1
    print(f"\nfail-first: every claimed test fails against {base_sha}")
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
