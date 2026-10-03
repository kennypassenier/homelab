#!/usr/bin/env python3
"""Register discipline: a row may not claim more than was measured.

fix-guards-2 and fix-guards-4 (Kenny, 2026-10-03: "je zou je gedragen als
een world-class developer, maar ik zie vaak amateuristische fouten, die
moeten eruit"). Counted in docs/deployment/REGISTER.md that day:

* 86 rows filed since 2026-09-26 say "done" with no live measurement in
  their status: 53 are "done 2026-10-02 (register audit): … Not re-measured
  one by one", the rest a bare "done" or "done <date>".
* 110 rows name a release that is long out ("doing: release 3.70.1",
  "built 2026-10-01, ships in 3.70.0", "released 3.70.7 … this row not
  measured on its own") and were never measured after it: every 3.70.x
  release went out on top of the previous one's unmeasured rows.
* redesign-backups-4 (and the older step-28) sit "open" with nobody named
  as holding them — a gap listed instead of built or decided.

Two modes, both plain python3 and git (the rest of .githooks needs nothing
more either):

  check-register.py                 the commit gate: reads the staged diff
                                    of REGISTER.md and judges only the rows
                                    this commit adds or changes, so history
                                    stays as it is and the next row cannot
                                    repeat the fault.
  check-register.py --release X.Y.Z the release gate (`make release`):
                                    refuses while any row of an EARLIER
                                    release is still unmeasured.
  --diff FILE / --register FILE     read these instead of git (tests);
  --known a,b / --invariants FILE   with --diff: the test names that exist,
                                    and an INVARIANTS.md to judge.

At commit it also refuses a changed row naming a test that does not exist
and a staged INVARIANTS.md whose numbering is broken (fix-guards-1, -3):
tests wait for the release in this repository, so the Rust checks that
read the documents would otherwise first speak at `make release`.

A deliberate release past unmeasured rows stays possible and visible:
UNMEASURED_OK="<why>" make release … prints the rows and the reason.
"""
import os
import re
import subprocess
import sys

REGISTER = "docs/deployment/REGISTER.md"

DATE = re.compile(r"\b20\d\d-\d\d-\d\d\b")
# "not measured", "not yet measured", "never measured", "not re-measured",
# "unmeasured", "niet gemeten" — a negated measurement is no measurement.
NEGATED = re.compile(
    r"\b(?:not|never|niet|nog niet)\s+(?:yet\s+|re-|been\s+|live\s+)*(?:measured|gemeten)\b"
    r"|\bun-?measured\b|\bre-measured\b",
    re.I,
)
MEASURED = re.compile(r"\b(?:measured|gemeten|nagemeten)\b", re.I)
RELEASE = re.compile(
    r"\b(?:release[sd]?|ships? in|shipped in|live in|built for)\s+v?(\d+\.\d+\.\d+)", re.I
)
CLOSED = ("done", "obsolete", "closed", "dropped", "superseded", "parked", "klopt")
HOLDER = re.compile(
    r"\bKenny\b|\bblocked\b|\bwaits? on\b|\bwaiting on\b|\bdecided\b|\bowner\b|\bLater\b", re.I
)
FAIL_FIRST = re.compile(
    r"fail(?:ed|s)?\s+first|fail-first|failed on the old|fails on the old|"
    r"failed against the old|sabotage",
    re.I,
)
ROW_ID = re.compile(r"^\| ([A-Za-z]+(?:-[a-z0-9]+)*-?\d+[a-z]?) \|")
TEST_NAME = re.compile(
    r"`((?:fix|feat|redesign|gap|step|ask|arch|tech|scope)_(?:[a-z]+_)?\d+_[a-z0-9_]+)`"
)


def cells(line):
    return [c.strip() for c in line.strip().strip("|").split("|")]


def status_of(line):
    for c in reversed(cells(line)):
        if c:
            return c
    return ""


def measured(text):
    """A date and a measurement word, and no measurement denied anywhere:
    "live on 3.70.7 (page version measured); this row not measured on its
    own" measured the release, not the row."""
    if NEGATED.search(text):
        return False
    return bool(DATE.search(text)) and bool(MEASURED.search(text))


def version_tuple(v):
    return tuple(int(x) for x in v.split("."))


def judge_changed_row(line, is_new, old_status=None):
    """Faults of one row this commit adds (is_new) or changes. The status
    rules judge a status this commit writes; a row touched only to correct
    its text (a renamed test) keeps the status it had."""
    rid = ROW_ID.match(line).group(1)
    st = status_of(line)
    low = st.lower()
    faults = []
    if old_status is not None and old_status == st:
        low = ""
    if low.startswith("done") and not measured(st):
        faults.append(
            f"{rid}: closed as done without a live measurement in its status. Say when and "
            f"what was measured (\"done 2026-10-03: measured <what>, with <command/page>\"); "
            f"a fix that is only released is \"released X.Y.Z; measure: <how, when>\", not done."
        )
    m = RELEASE.search(st)
    if m and low.startswith("released") and not measured(st) and "measure:" not in low:
        faults.append(
            f"{rid}: released {m.group(1)} but neither measured nor saying how it will be "
            f"(add \"measure: <what to read, where, when>\")."
        )
    if re.match(r"(open|later|parked)\b", low) and not HOLDER.search(st):
        faults.append(
            f"{rid}: left {low.split()[0]} with nobody holding it. Build it now, or name who "
            f"decided it waits and on what (\"open: Kenny 2026-10-03, Later\", "
            f"\"open: blocked on <x>\")."
        )
    if is_new and TEST_NAME.search(line) and not FAIL_FIRST.search(line):
        faults.append(
            f"{rid}: names its tests but not that they failed first against the old code. "
            f"Run them on the old code (`make fail-first`) and say so (\"failed first: …\")."
        )
    return faults


def missing_tests(line, exists):
    """Test names a row cites that `exists` cannot find, unless the row
    marks them `(gone: …)` or `(in another repository…)` after the name."""
    out = []
    for m in TEST_NAME.finditer(line):
        tail = line[m.end():]
        if "(gone:" in tail or "(in another repository" in tail:
            continue
        if not exists(m.group(1)):
            out.append(m.group(1))
    return out


def staged_test_exists(name):
    """Is `name` a Rust test fn or a node test in the staged tree?"""
    rust = subprocess.run(
        ["git", "grep", "--cached", "-q", "-F", f"fn {name}(", "--", "*.rs"],
        capture_output=True, check=False,
    )
    if rust.returncode == 0:
        return True
    node = subprocess.run(
        ["git", "grep", "--cached", "-q", "-F", name, "--", "*.test.js", "*.e2e.js"],
        capture_output=True, check=False,
    )
    return node.returncode == 0


def numbering_faults(doc):
    """INVARIANTS.md's table: one unbroken table, every number once, 1..N."""
    lines = doc.splitlines()
    header = next((i for i, l in enumerate(lines) if l.lstrip().startswith("| # |")), None)
    if header is None:
        return ["docs/INVARIANTS.md: no `| # |` table header"]
    num = re.compile(r"^\|\s*(\d+)\s*\|")
    last = max((i for i, l in enumerate(lines) if num.match(l.strip())), default=header)
    faults, seen, prev = [], {}, None
    for i in range(header + 2, last + 1):
        m = num.match(lines[i].strip())
        if not m:
            faults.append(f"docs/INVARIANTS.md line {i + 1}: the table is broken here (a blank or non-row line between rows)")
            continue
        n = int(m.group(1))
        if n in seen:
            faults.append(f"docs/INVARIANTS.md: row {n} appears twice (lines {seen[n]} and {i + 1}) — give the later one the next free number and move its citations with it")
        elif prev is not None and n != prev + 1:
            faults.append(f"docs/INVARIANTS.md line {i + 1}: row {n} follows row {prev}")
        seen.setdefault(n, i + 1)
        prev = n
    return faults


def commit_mode(diff_text, exists=None):
    removed = {}
    added = []
    for raw in diff_text.splitlines():
        if raw.startswith("---") or raw.startswith("+++"):
            continue
        if raw.startswith("-"):
            m = ROW_ID.match(raw[1:])
            if m:
                removed[m.group(1)] = status_of(raw[1:])
        elif raw.startswith("+"):
            m = ROW_ID.match(raw[1:])
            if m:
                added.append(raw[1:])
    faults = []
    for line in added:
        rid = ROW_ID.match(line).group(1)
        faults += judge_changed_row(line, is_new=rid not in removed, old_status=removed.get(rid))
        if exists is not None:
            for name in missing_tests(line, exists):
                faults.append(
                    f"{rid}: names the test `{name}`, which does not exist (renamed? deleted?). "
                    f"Fix the name, or write `(gone: <why, commit>)` after it."
                )
    return faults


def release_mode(register_text, version):
    """Rows of a release before `version` that are still unmeasured."""
    target = version_tuple(version)
    out = []
    for line in register_text.splitlines():
        m = ROW_ID.match(line)
        if not m:
            continue
        st = status_of(line)
        low = st.lower()
        if low.startswith(CLOSED):
            continue
        rel = RELEASE.search(st)
        if not rel or version_tuple(rel.group(1)) >= target:
            continue
        if measured(st):
            continue
        out.append(f"{m.group(1)} ({rel.group(1)}): {st[:110]}")
    return out


def main(argv):
    args = argv[1:]
    def opt(name):
        if name in args:
            i = args.index(name)
            return args[i + 1]
        return None

    version = opt("--release")
    if version:
        path = opt("--register") or REGISTER
        text = open(path, encoding="utf-8").read()
        rows = release_mode(text, version)
        if not rows:
            print(f"register: every row of a release before {version} is measured or closed")
            return 0
        print(
            f"RELEASE BLOCKED — {len(rows)} register row(s) of a release before {version} "
            f"were never measured after it went out:",
            file=sys.stderr,
        )
        for r in rows:
            print(f"  {r}", file=sys.stderr)
        print(
            "Measure each live and write \"done <date>: measured <what>\", or close it as "
            "obsolete/superseded with the reason. A release on top of unmeasured rows "
            "is how 3.70.1 to 3.70.7 piled up; to go ahead anyway, deliberately: "
            "UNMEASURED_OK=\"<why>\" make release …",
            file=sys.stderr,
        )
        why = os.environ.get("UNMEASURED_OK", "").strip()
        if why:
            print(f"register: going ahead anyway, UNMEASURED_OK: {why}", file=sys.stderr)
            return 0
        return 1

    # Tests wait for the release here (Kenny, 2026-09-30), so the Rust
    # checks that read these documents do not run at commit; the two
    # cheapest of them run here instead, on what this commit stages.
    path = opt("--diff")
    if path:
        diff_text = open(path, encoding="utf-8").read()
        known = set(filter(None, (opt("--known") or "").split(",")))
        exists = known.__contains__
        inv_path = opt("--invariants")
        invariants = open(inv_path, encoding="utf-8").read() if inv_path else None
    else:
        diff_text = subprocess.run(
            ["git", "diff", "--cached", "-U0", "--", REGISTER],
            capture_output=True, text=True, check=False,
        ).stdout
        exists = staged_test_exists
        staged = subprocess.run(
            ["git", "diff", "--cached", "--name-only"], capture_output=True, text=True, check=False
        ).stdout.split()
        invariants = None
        if "docs/INVARIANTS.md" in staged:
            invariants = subprocess.run(
                ["git", "show", ":docs/INVARIANTS.md"], capture_output=True, text=True, check=False
            ).stdout
    faults = commit_mode(diff_text, exists)
    if invariants is not None:
        faults += numbering_faults(invariants)
    if faults:
        print("COMMIT BLOCKED — the register or INVARIANTS.md claims more than holds:", file=sys.stderr)
        for f in faults:
            print(f"  {f}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
