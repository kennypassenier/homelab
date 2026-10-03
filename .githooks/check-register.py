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

The status shapes this gate accepts when a commit writes a status:

  done <date>: measured <what, how>        closed, with the measurement
  done <date>: `<command>` <what it read>  the clause starts right after
                                           the prefix; a negation in that
                                           clause ("will be measured") is
                                           no measurement
  measure-after <date>: <how>              released, measured on or after
                                           <date>; the release gate lets it
                                           wait until then, never longer
  proven <date>: <tests> passed on the demo host (<duration>)
                                           released, and proven by tests
                                           instead of live: every test it
                                           names (`fn_name`, or
                                           `file.e2e.js: "case title"`) must
                                           exist, "passed" and the run's
                                           measured duration are said, and
                                           the word "measured" is not (that
                                           is for a live measurement)
  obsolete|superseded|closed|dropped|klopt <date>: <reason, 20+ chars>
  open|later|parked: <who> <date>, <why>   a gap someone holds
  anything else (doing: release 3.71.0, built …) is work in progress.

Modes, all plain python3 and git (the rest of .githooks needs nothing more):

  check-register.py                 the commit gate: the staged REGISTER.md
                                    rows this commit adds or changes, a
                                    staged INVARIANTS.md's numbering, and
                                    register ids in staged user-facing
                                    strings. In a merge or cherry-pick only
                                    rows that differ from EVERY parent are
                                    judged: the others were judged where
                                    they were written.
  check-register.py --release X.Y.Z the release gate (`make release`):
                                    refuses while a row of an earlier
                                    release is unmeasured, grouped by
                                    release and kind. UNMEASURED_OK="<why>"
                                    goes ahead; --record FILE writes the
                                    reason and the rows for the tag.
  check-register.py --tree WHAT     the documents as they are on disk (the
                                    Rust tests call this, so one source
                                    judges): WHAT is tests, dates or
                                    numbering.
  --diff FILE / --register FILE / --invariants FILE / --known a,b /
  --today YYYY-MM-DD / --root DIR   read these instead of git (tests).
"""
import datetime
import os
import re
import subprocess
import sys

REGISTER = "docs/deployment/REGISTER.md"
INVARIANTS = "docs/INVARIANTS.md"
CORRECTIONS = "docs/deployment/CORRECTIONS.md"
HERE = os.path.dirname(os.path.abspath(__file__))

DATE_S = r"20\d\d-\d\d-\d\d"
DATE = re.compile(rf"(?<!\d){DATE_S}(?!\d)")
# fix-guards (M6): every row of a register table, whatever its id's shape:
# fix-240, feat-shell-1, F186, M-G7, tui-host-settings.
ROW_ID = re.compile(r"^\| ([A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*) \|")
# M5: a cell may hold an escaped pipe.
PIPE = re.compile(r"(?<!\\)\|")

CLOSED_WORDS = ("obsolete", "superseded", "closed", "dropped", "klopt")
CLOSED = ("done",) + CLOSED_WORDS
DONE = re.compile(rf"^done ({DATE_S}):\s*(.*)$", re.I | re.S)
REASONED = re.compile(rf"^({'|'.join(CLOSED_WORDS)}) ({DATE_S}):\s*(.+)$", re.I | re.S)
MEASURE_AFTER = re.compile(rf"^measure-after ({DATE_S}):\s*(.+)$", re.I | re.S)
# measure2 (Kenny, 2026-10-03: only rows that truly need him may reach the
# 3.71.0 Go form): a released row whose behaviour a whole-screen test on
# the demo host, or a core/host test on the mock executor, pins — proven,
# never called measured.
PROVEN = re.compile(rf"^proven ({DATE_S}):\s*(.+)$", re.I | re.S)
PROVEN_DURATION = re.compile(r"\([^)]*?\d+(?:\.\d+)?\s*(?:ms|s|min)\b[^)]*\)")
BACKTICK = re.compile(r"`([^`]+)`")
JS_REF = re.compile(r'^([\w./-]+\.js):\s*"(.+)"$', re.S)
RUST_REF = re.compile(r"^[a-z][a-z0-9]*(?:_[a-z0-9]+){2,}$")
HELD = re.compile(rf"^(open|later|parked):\s*(\S.*?)\s+({DATE_S}),\s*(\S.+)$", re.I | re.S)
OPENISH = re.compile(r"^(open|later|parked)\b", re.I)

# M1: a measurement clause starts with the measurement itself.
CLAUSE_END = re.compile(r";|\.\s|\s—\s|,?\s+but\s")
MEASURE_START = re.compile(r"^(?:re-)?measured\b|^`", re.I)
NEGATION = re.compile(
    r"\b(?:not|never|niet|nog niet|wasn't|was not|isn't|is not|will be|to be|yet to be|"
    r"cannot be|can't be|couldn't be|could not be)\s+(?:yet\s+|re-|been\s+|live\s+|na)*"
    r"(?:measured|gemeten)\b|\bun-?measured\b",
    re.I,
)

# M2: fail-first said positively; a negation right before it voids it.
FAIL_FIRST = re.compile(
    r"\bfail(?:ed|s)? first\b|\bfailed (?:on|against) (?:the )?(?:old|previous) code\b|"
    r"\bfailed on a constructed bad case\b|\bfailed under sabotage\b",
    re.I,
)
FAIL_FIRST_NEG = re.compile(r"\b(?:no|not|never|without|didn't|did not|niet|geen)\b[^.;]*$", re.I)

# LOW: a homelab version, not another project's. The version follows a
# release word or a `v`; the word before that may not name a sibling
# project (a hyphenated name, a CamelCase name, or one of the plain names
# whose versions share homelab's 3.x and 4.x range).
OTHER_PROJECTS = {"kyu", "almanac", "latch", "chassis", "newsflash", "docgen", "jellyfin"}
RELEASE = re.compile(
    r"(?:(?P<pre>[\w.-]+)\s+)?"
    r"(?:(?:release[sd]?|ships? in|shipped in|live (?:in|on)|built for|rollout of|rolled out)"
    r"\s+v?|\bv)(?P<ver>\d+\.\d+\.\d+)(?![.\d])",
    re.I,
)

TEST_NAME = re.compile(
    r"`((?:fix|feat|redesign|gap|step|ask|arch|tech|scope)_(?:[a-z]+_)?\d+_[a-z0-9_]+)`"
)


def kinds():
    """(lowercase kinds, uppercase prefixes) of a register id: the one list
    the Python, Rust and JavaScript id checks read."""
    lower, upper = [], []
    with open(os.path.join(HERE, "register-id-kinds.txt"), encoding="utf-8") as f:
        for line in f:
            line = line.split("#", 1)[0].strip()
            if not line:
                continue
            (upper if line.isupper() else lower).append(line)
    return lower, upper


def id_pattern():
    lower, upper = kinds()
    return re.compile(
        rf"(?<![\w-])(?:(?:{'|'.join(lower)})-(?:[a-z]+-)*\d+|(?:{'|'.join(upper)})\d+)"
        rf"(?![\w.-]*\d)(?![\w-])"
    )


def cells(line):
    s = line.strip()
    if s.startswith("|"):
        s = s[1:]
    if s.endswith("|") and not s.endswith("\\|"):
        s = s[:-1]
    return [c.strip() for c in PIPE.split(s)]


def status_of(line):
    for c in reversed(cells(line)):
        if c:
            return c
    return ""


def clauses(text):
    return [c.strip() for c in CLAUSE_END.split(text)]


def is_measurement(clause):
    return bool(MEASURE_START.match(clause)) and not NEGATION.search(clause)


def measured(status):
    """A dated status holding a clause that IS a measurement: "done
    2026-10-03: measured …", "released 3.70.7; measured 2026-10-03 with
    `homelab doctor`". A clause that only mentions measuring ("page version
    measured", "this row not measured on its own") is not one."""
    m = DONE.match(status)
    if m:
        return is_measurement(clauses(m.group(2))[0])
    if not DATE.search(status):
        return False
    return any(
        is_measurement(c.strip()) for c in re.split(r";|\.\s|:\s|\s—\s|,?\s+but\s", status)
    )


def homelab_versions(text):
    out = []
    for m in RELEASE.finditer(text):
        pre = (m.group("pre") or "").strip(".,;:()")
        foreign = (
            pre.lower() in OTHER_PROJECTS
            or re.fullmatch(r"[A-Za-z][A-Za-z0-9]*(?:-[A-Za-z][A-Za-z0-9]*)+", pre) is not None
            or re.fullmatch(r"[A-Z][a-z0-9]+[A-Z]\w*|[A-Z]{3,}\w*[a-z]\w*", pre) is not None
        )
        if not foreign:
            out.append(m.group("ver"))
    return out


def version_tuple(v):
    return tuple(int(x) for x in v.split("."))


def parse_date(s):
    return datetime.date.fromisoformat(s)


def judge_status(rid, st, today):
    """Faults of a status a commit writes."""
    low = st.lower()
    faults = []
    if low.startswith("done"):
        m = DONE.match(st)
        if not m or not is_measurement(clauses(m.group(2))[0]):
            faults.append(
                f"{rid}: closed as done without a live measurement right after the prefix. "
                f"Write \"done <date>: measured <what>, with <command/page>\" or "
                f"\"done <date>: `<command>` <what it read>\"; a fix that is only released is "
                f"\"measure-after <date>: <how>\", not done."
            )
    elif low.startswith("measure-after"):
        m = MEASURE_AFTER.match(st)
        if not m or len(m.group(2).strip()) < 10:
            faults.append(
                f"{rid}: write \"measure-after <date>: <what to read, where>\" "
                f"(the date it can first be measured, and how)."
            )
        elif parse_date(m.group(1)) < today:
            faults.append(
                f"{rid}: measure-after {m.group(1)} lies in the past — measure it now "
                f"(\"done <date>: measured …\")."
            )
    elif low.startswith("proven"):
        faults += proven_faults(rid, st, None, None)
    elif low.startswith(CLOSED_WORDS):
        m = REASONED.match(st)
        if not m or len(m.group(3).strip()) < 20:
            word = low.split()[0].rstrip(":")
            faults.append(
                f"{rid}: closed as {word} without a dated reason. Write "
                f"\"{word} <date>: <why, at least 20 characters>\"."
            )
    elif OPENISH.match(low):
        if not HELD.match(st):
            word = OPENISH.match(low).group(1)
            faults.append(
                f"{rid}: left {word} with nobody holding it. Build it now, or write "
                f"\"{word}: <who> <date>, <why it waits>\" (\"open: Kenny 2026-10-03, "
                f"waits on the drill decision\")."
            )
    elif low.startswith("released") and not measured(st):
        faults.append(
            f"{rid}: released but not measured. Write \"measure-after <date>: <how>\" "
            f"until it can be, then \"done <date>: measured …\"."
        )
    return faults


def proven_refs(body):
    """The tests a `proven` status names: ("name", fn_or_test_name) for a
    backticked snake_case name, ("js", path, title) for
    `file.e2e.js: "case title"`. Other backticked text (a command) is no
    test."""
    out = []
    for span in BACKTICK.findall(body):
        span = span.strip()
        m = JS_REF.match(span)
        if m:
            out.append(("js", m.group(1), m.group(2)))
        elif RUST_REF.match(span):
            out.append(("name", span))
    return out


def proven_faults(rid, st, exists, js_exists):
    """Faults of a `proven` status: its shape, and every test it names
    existing (`exists`/`js_exists` None: shape only)."""
    m = PROVEN.match(st)
    if not m:
        return [f"{rid}: write \"proven <date>: <tests> passed on the demo host (<duration>)\"."]
    body = m.group(2)
    refs = proven_refs(body)
    faults = []
    if not refs:
        faults.append(
            f"{rid}: proven by no named test. Name each one: `fn_name`, or "
            f"`admin/web/test-e2e/invariants.e2e.js: \"<case title>\"`."
        )
    if not re.search(r"\bpassed\b", body, re.I) or not PROVEN_DURATION.search(body):
        faults.append(f"{rid}: a proven row says the tests passed and how long the run took (\"passed … (41 s)\").")
    if re.search(r"\bmeasured\b", BACKTICK.sub("", body), re.I):
        faults.append(
            f"{rid}: a test-proven row never says measured; that word is for a live measurement "
            f"(\"done <date>: measured …\")."
        )
    for ref in refs:
        if ref[0] == "name" and exists is not None and not exists(ref[1]):
            faults.append(f"{rid}: `{ref[1]}` does not exist (renamed? deleted?); it is this row's proof.")
        if ref[0] == "js" and js_exists is not None and not js_exists(ref[1], ref[2]):
            faults.append(f"{rid}: does not exist in {ref[1]}: the case \"{ref[2][:70]}\", this row's proof.")
    return faults


def says_failed_first(line):
    for m in FAIL_FIRST.finditer(line):
        before = line[max(0, m.start() - 30):m.start()]
        if not FAIL_FIRST_NEG.search(before):
            return True
    return False


def judge_changed_row(line, is_new, old_status=None, today=None):
    """Faults of one row this commit adds (is_new) or changes. The status
    rules judge a status this commit writes; a row touched only to correct
    its text (a renamed test) keeps the status it had."""
    today = today or datetime.date.today()
    rid = ROW_ID.match(line).group(1)
    st = status_of(line)
    faults = []
    if old_status is None or old_status != st:
        faults += judge_status(rid, st, today)
        for f in future_dates(st, today):
            faults.append(f"{rid}: {f}")
    if is_new and TEST_NAME.search(line) and not says_failed_first(line):
        faults.append(
            f"{rid}: names its tests but not that they failed first against the old code. "
            f"Run them on the old code (`make fail-first`) and say so (\"failed first: …\")."
        )
    return faults


def future_dates(text, today):
    """Every date in `text` that is impossible or lies after `today`; a date
    right after `measure-after` is a plan, not a measurement."""
    out = []
    for m in DATE.finditer(text):
        s = m.group(0)
        planned = re.search(r"measure-after\s*$", text[:m.start()], re.I)
        try:
            d = parse_date(s)
        except ValueError:
            out.append(f"{s} is not a date")
            continue
        if d > today and not planned:
            out.append(f"{s} lies in the future")
    return out


def missing_tests(line, exists):
    """Test names a row cites that `exists` cannot find, unless the row
    marks that name `(gone: …)` or `(in another repository…)` right after
    its closing backtick."""
    out = []
    for m in TEST_NAME.finditer(line):
        tail = line[m.end():].lstrip()
        if tail.startswith("(gone:") or tail.startswith("(in another repository"):
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


def staged_js_case_exists(path, title):
    r = subprocess.run(
        ["git", "grep", "--cached", "-q", "-F", "-e", title, "--", f"*{path}"],
        capture_output=True, check=False,
    )
    return r.returncode == 0


def tree_js_case_exists(root):
    cache = {}

    def exists(path, title):
        if path not in cache:
            texts = []
            for d, dirs, files in os.walk(root):
                dirs[:] = [x for x in dirs if not (x.startswith("target") or x in (".git", ".claude", "node_modules"))]
                for f in files:
                    full = os.path.join(d, f)
                    if f.endswith(".js") and full.endswith("/" + path.lstrip("/")) or f == path:
                        texts.append(open(full, encoding="utf-8", errors="replace").read())
            cache[path] = "\n".join(texts)
        return title in cache[path]

    return exists


def tree_test_exists(root):
    rust, js = [], []
    for d, dirs, files in os.walk(root):
        dirs[:] = [x for x in dirs if not (x.startswith("target") or x in (".git", ".claude", "node_modules"))]
        for f in files:
            if f.endswith(".rs"):
                rust.append(os.path.join(d, f))
            elif f.endswith(".test.js") or f.endswith(".e2e.js"):
                js.append(os.path.join(d, f))
    rust_text = "\n".join(open(p, encoding="utf-8", errors="replace").read() for p in rust)
    js_text = "\n".join(open(p, encoding="utf-8", errors="replace").read() for p in js)
    return lambda name: f"fn {name}(" in rust_text or name in js_text


def numbering_faults(doc):
    """INVARIANTS.md's table: one unbroken table, every number once, 1..N.
    The table is the header and every `|` line contiguous with it; a
    numbered row found after it means the table was cut."""
    lines = doc.splitlines()
    header = next((i for i, l in enumerate(lines) if l.lstrip().startswith("| # |")), None)
    if header is None:
        return ["docs/INVARIANTS.md: no `| # |` table header"]
    num = re.compile(r"^\|\s*(\d+)\s*\|")
    end = header + 2
    while end < len(lines) and lines[end].lstrip().startswith("|"):
        end += 1
    faults, seen, prev = [], {}, None
    for i in range(header + 2, end):
        m = num.match(lines[i].strip())
        if not m:
            faults.append(f"docs/INVARIANTS.md line {i + 1}: a row without a number inside the table")
            continue
        n = int(m.group(1))
        if n in seen:
            faults.append(f"docs/INVARIANTS.md: row {n} appears twice (lines {seen[n]} and {i + 1}) — give the later one the next free number and move its citations with it")
        elif prev is None and n != 1:
            faults.append(f"docs/INVARIANTS.md: the first row is {n}, not 1")
        elif prev is not None and n != prev + 1:
            faults.append(f"docs/INVARIANTS.md line {i + 1}: row {n} follows row {prev}")
        seen.setdefault(n, i + 1)
        prev = n
    stray = next((i for i in range(end, len(lines)) if num.match(lines[i].strip())), None)
    if stray is not None:
        faults.append(
            f"docs/INVARIANTS.md line {end + 1}: the table is broken here (a blank or non-row "
            f"line between rows); row {num.match(lines[stray].strip()).group(1)} at line "
            f"{stray + 1} renders as plain text"
        )
    return faults


def rows_of(text):
    return {l for l in text.splitlines() if ROW_ID.match(l)}


def commit_mode(diff_text, exists=None, parents=None, today=None, js_exists=None):
    """`parents`: the REGISTER.md text of every parent of a merge or
    cherry-pick; a row present verbatim in one of them is not judged."""
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
    known_rows = set()
    for p in parents or []:
        known_rows |= rows_of(p)
    faults = []
    for line in added:
        if line in known_rows:
            continue
        rid = ROW_ID.match(line).group(1)
        faults += judge_changed_row(
            line, is_new=rid not in removed, old_status=removed.get(rid), today=today
        )
        st = status_of(line)
        if st.lower().startswith("proven") and removed.get(rid) != st:
            faults += [f for f in proven_faults(rid, st, exists, js_exists) if "does not exist" in f]
        if exists is not None:
            for name in missing_tests(line, exists):
                faults.append(
                    f"{rid}: names the test `{name}`, which does not exist (renamed? deleted?). "
                    f"Fix the name, or write `(gone: <why, commit>)` right after it."
                )
    return faults


STRING = {
    ".rs": re.compile(r'"(?:[^"\\]|\\.)*"'),
    ".js": re.compile(r'"(?:[^"\\]|\\.)*"|\'(?:[^\'\\]|\\.)*\'|`(?:[^`\\]|\\.)*`'),
}


def ids_in_added_strings(diff_text):
    """H4: register ids in string literals a staged diff adds to the code a
    person reads (Rust under */src/, the dashboard's js). Line-based and
    cheap; the full lexers run in the test suite."""
    pat = id_pattern()
    out = []
    path = None
    for raw in diff_text.splitlines():
        if raw.startswith("+++ "):
            p = raw[4:].strip()
            path = p[2:] if p.startswith("b/") else None
            continue
        if not raw.startswith("+") or raw.startswith("+++") or path is None:
            continue
        ext = os.path.splitext(path)[1]
        if ext not in STRING:
            continue
        if ext == ".rs" and ("/src/" not in path or "/tests/" in path):
            continue
        if ext == ".js" and not path.startswith("admin/web/js/"):
            continue
        line = raw[1:]
        code = line.strip()
        if code.startswith(("//", "*", "/*")) or "id-ok:" in line:
            continue
        for lit in STRING[ext].findall(line):
            text = re.sub(r"\[[^\]]*\]", "", lit[1:-1])
            if not re.search(r"\s", text.strip()):
                continue
            m = pat.search(text)
            if m:
                out.append(f"{path}: the text {lit[:80]} names the register id {m.group(0)} — say what it does instead (`// id-ok: <why>` if it must)")
    return out


def group_rows(rows):
    """{release: {kind: [line]}} for the release gate's listing."""
    out = {}
    for rid, ver, st in rows:
        kind = re.sub(r"-?\d+[a-z]?$", "", rid) or rid
        out.setdefault(ver, {}).setdefault(kind, []).append(f"{rid}: {st[:110]}")
    return out


def release_mode(register_text, version, today, exists=None, js_exists=None):
    """(row id, release, status) of every row of a release before `version`
    that is not measured, of every `measure-after` row whose date has
    come, and of every `proven` row whose tests are gone."""
    target = version_tuple(version)
    out = []
    for line in register_text.splitlines():
        m = ROW_ID.match(line)
        if not m:
            continue
        st = status_of(line)
        low = st.lower()
        if low.startswith("proven"):
            faults = proven_faults(m.group(1), st, exists, js_exists)
            if faults:
                out.append((m.group(1), "proven by a test that does not hold", faults[0].split(": ", 1)[1]))
            continue
        if low.startswith(CLOSED):
            continue
        after = MEASURE_AFTER.match(st)
        vers = homelab_versions(st)
        if after:
            if parse_date(after.group(1)) > today:
                continue
            out.append((m.group(1), max(vers, key=version_tuple) if vers else "measure-after due", st))
            continue
        if not vers:
            continue
        newest = max(vers, key=version_tuple)
        if version_tuple(newest) >= target or measured(st):
            continue
        out.append((m.group(1), newest, st))
    return out


def git_text(*args):
    r = subprocess.run(["git", *args], capture_output=True, text=True, check=False)
    return r.stdout if r.returncode == 0 else None


class Usage(Exception):
    pass


def main(argv):
    args = argv[1:]

    def opt(name):
        if name not in args:
            return None
        i = args.index(name)
        if i + 1 >= len(args) or args[i + 1].startswith("--"):
            raise Usage(f"{name} needs a value")
        return args[i + 1]

    try:
        version = opt("--release")
        today_s = opt("--today")
        today = parse_date(today_s) if today_s else datetime.date.today()
        if version is not None and not re.fullmatch(r"\d+\.\d+\.\d+", version):
            raise Usage(f"--release takes a plain x.y.z version, not {version!r}")
        tree = opt("--tree")
    except (Usage, ValueError) as e:
        print(f"check-register.py: {e}", file=sys.stderr)
        return 2

    if version:
        path = opt("--register") or REGISTER
        text = open(path, encoding="utf-8").read()
        known = opt("--known")
        if known is not None:
            names = set(filter(None, known.split(",")))
            exists, js_exists = names.__contains__, (lambda _p, t: t in names)
        else:
            root = opt("--root") or (git_text("rev-parse", "--show-toplevel") or ".").strip()
            exists, js_exists = tree_test_exists(root), tree_js_case_exists(root)
        rows = release_mode(text, version, today, exists, js_exists)
        if not rows:
            print(f"register: every row of a release before {version} is measured or closed")
            return 0
        lines = []
        def order(kv):
            ver = kv[0]
            return (0, version_tuple(ver)) if ver[0].isdigit() else (1, ())

        for ver, by_kind in sorted(group_rows(rows).items(), key=order):
            n = sum(len(v) for v in by_kind.values())
            lines.append(f"{ver} — {n} row(s)")
            for kind, items in sorted(by_kind.items()):
                lines.append(f"  {kind} ({len(items)})")
                lines += [f"    {r}" for r in items]
        print(
            f"RELEASE BLOCKED — {len(rows)} register row(s) of a release before {version} "
            f"were never measured after it went out, by release and kind:",
            file=sys.stderr,
        )
        for l in lines:
            print(f"  {l}", file=sys.stderr)
        print(
            "Measure each live and write \"done <date>: measured <what>\"; one that cannot be "
            "measured yet gets \"measure-after <date>: <how>\"; one that no longer applies "
            "\"obsolete <date>: <why>\". A release on top of unmeasured rows is how 3.70.1 to "
            "3.70.7 piled up; to go ahead anyway, deliberately: UNMEASURED_OK=\"<why>\" make release …",
            file=sys.stderr,
        )
        why = os.environ.get("UNMEASURED_OK", "").strip()
        if not why:
            return 1
        print(f"register: going ahead anyway, UNMEASURED_OK: {why}", file=sys.stderr)
        record = opt("--record")
        if record:
            with open(record, "a", encoding="utf-8") as f:
                f.write(f"Released past {len(rows)} unmeasured register row(s). UNMEASURED_OK: {why}\n")
                f.writelines(f"{l}\n" for l in lines)
        return 0

    if tree:
        root = opt("--root") or git_text("rev-parse", "--show-toplevel").strip()
        faults = []
        if tree in ("tests", "all"):
            exists = tree_test_exists(root)
            for line in open(os.path.join(root, REGISTER), encoding="utf-8").read().splitlines():
                m = ROW_ID.match(line)
                if m:
                    faults += [f"{m.group(1)}: `{n}`" for n in missing_tests(line, exists)]
        if tree in ("dates", "all"):
            # One day of slack: a status written just after midnight here
            # is still yesterday in UTC.
            slack = today + datetime.timedelta(days=1)
            for line in open(os.path.join(root, REGISTER), encoding="utf-8").read().splitlines():
                m = ROW_ID.match(line)
                if m:
                    faults += [f"REGISTER {m.group(1)}: {f}" for f in future_dates(status_of(line), slack)]
            for line in open(os.path.join(root, CORRECTIONS), encoding="utf-8").read().splitlines():
                if line.startswith("## ") or "Ratified" in line:
                    faults += [f"CORRECTIONS `{line.strip()[:80]}`: {f}" for f in future_dates(line, slack)]
        if tree in ("numbering", "all"):
            faults += numbering_faults(open(os.path.join(root, INVARIANTS), encoding="utf-8").read())
        for f in faults:
            print(f, file=sys.stderr)
        return 1 if faults else 0

    # Tests wait for the release here (Kenny, 2026-09-30), so the Rust
    # checks that read these documents do not run at commit; the cheap
    # ones run here instead, on what this commit stages.
    try:
        path = opt("--diff")
    except Usage as e:
        print(f"check-register.py: {e}", file=sys.stderr)
        return 2
    parents = None
    code_diff = ""
    if path:
        diff_text = open(path, encoding="utf-8").read()
        known = set(filter(None, (opt("--known") or "").split(",")))
        exists = known.__contains__
        js_exists = lambda _p, t: t in known
        inv_path = opt("--invariants")
        invariants = open(inv_path, encoding="utf-8").read() if inv_path else None
        code_path = opt("--code-diff")
        code_diff = open(code_path, encoding="utf-8").read() if code_path else ""
    else:
        diff_text = git_text("diff", "--cached", "-U0", "--", REGISTER) or ""
        exists = staged_test_exists
        js_exists = staged_js_case_exists
        staged = (git_text("diff", "--cached", "--name-only") or "").split()
        invariants = git_text("show", f":{INVARIANTS}") if INVARIANTS in staged else None
        code_diff = git_text("diff", "--cached", "-U0", "--", "*.rs", "admin/web/js/*.js") or ""
        # H3: a merge or cherry-pick brings rows written (and judged)
        # elsewhere; only what this commit itself writes is judged.
        git_dir = git_text("rev-parse", "--git-dir").strip()
        for head in ("MERGE_HEAD", "CHERRY_PICK_HEAD"):
            if os.path.exists(os.path.join(git_dir, head)):
                parents = [git_text("show", f"HEAD:{REGISTER}") or ""]
                for sha in open(os.path.join(git_dir, head)).read().split():
                    parents.append(git_text("show", f"{sha}:{REGISTER}") or "")
    faults = commit_mode(diff_text, exists, parents=parents, today=today, js_exists=js_exists)
    if invariants is not None:
        faults += numbering_faults(invariants)
    faults += ids_in_added_strings(code_diff)
    if faults:
        print("COMMIT BLOCKED — the register, INVARIANTS.md or a user-facing text claims more than holds:", file=sys.stderr)
        for f in faults:
            print(f"  {f}", file=sys.stderr)
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv))
