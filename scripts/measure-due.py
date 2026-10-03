#!/usr/bin/env python3
"""measure-due.py — measure the register rows whose `measure-after` date has
come, with the one read-only command each row names.

Why (Kenny, 2026-10-03): 76 rows of shipped releases reached the 3.71.0 Go
form as "could not be measured live". Most only needed a natural event —
the next nightly, the first deploy after a release. A row like that says
when and how, and this script does the reading once the day has come, so
a measurement never waits on somebody remembering it.

A row it can run has this status (docs/deployment/REGISTER.md):

  measure-after <date>: `<read-only command>` <what to look for>; expect /<regex>/

* exactly ONE backticked command, from the read-only allowlist below
  (`homelab snapshots …`, `homelab doctor`, `homelab status --json`, …);
  anything else is never run, the row is listed as needs-eyes;
* `expect /<regex>/`: the command's output must match it (Python `re`,
  multiline). No `expect`: needs-eyes, with the output's tail shown.

Verdicts per due row: pass (the regex matched), fail (it did not, or the
command failed), needs-eyes (nothing to run or nothing to match against).

Usage:
  scripts/measure-due.py            list the due rows and their commands
  scripts/measure-due.py --all      list every measure-after row, due or not
  scripts/measure-due.py --run      run each due row's command, judge it
  scripts/measure-due.py --run --write
                                    and rewrite each passed row to
                                    "done <today>: `<command>` <what it read>"
                                    in REGISTER.md (commit it yourself)
  --register FILE / --today YYYY-MM-DD / --homelab PATH   (tests)

`make release` runs `--run` before the register gate: a due row that is not
rewritten to done blocks the release (exit 1), exactly like the register
gate does; UNMEASURED_OK="<why>" goes past both, on the record.
Nothing here writes to a machine: every allowed command is a read.
"""
import datetime
import os
import re
import shlex
import subprocess
import sys

REGISTER = "docs/deployment/REGISTER.md"
DATE_S = r"20\d\d-\d\d-\d\d"
ROW_ID = re.compile(r"^\| ([A-Za-z][A-Za-z0-9]*(?:-[A-Za-z0-9]+)*) \|")
PIPE = re.compile(r"(?<!\\)\|")
MEASURE_AFTER = re.compile(rf"^measure-after ({DATE_S}):\s*(.+)$", re.I | re.S)
BACKTICK = re.compile(r"`([^`]+)`")
EXPECT = re.compile(r"\bexpect /(.+)/\s*$", re.S)

# The verbs that only read (client/src/main.rs). A row's command must be one
# of these, word for word up to the arguments shown, and carry no shell
# syntax: it is run without a shell.
READ_ONLY = [
    ("homelab", "ping"),
    ("homelab", "today"),
    ("homelab", "check"),
    ("homelab", "doctor"),
    ("homelab", "status"),
    ("homelab", "checks"),
    ("homelab", "incidents"),
    ("homelab", "snapshots"),
    ("homelab", "token", "list"),
    ("homelab", "config"),
    ("homelab", "templates"),
    ("homelab", "presets"),
    ("homelab", "ui", "state"),
]
SHELL_SYNTAX = re.compile(r"[;&|<>$`\\()]")


def cells(line):
    s = line.strip().strip("|")
    return [c.strip() for c in PIPE.split(s)]


def status_of(line):
    for c in reversed(cells(line)):
        if c:
            return c
    return ""


def read_only(cmd):
    """The argv of `cmd` when it is an allowed read, else None."""
    if SHELL_SYNTAX.search(cmd):
        return None
    try:
        argv = shlex.split(cmd)
    except ValueError:
        return None
    if argv and argv[0] == "homelab" and "checks" in argv[1:2] and len(argv) > 2:
        return None  # `homelab checks answer …` writes
    if argv and argv[1:2] == ["incidents"] and len(argv) > 2 and argv[2] != "show":
        return None
    for allowed in READ_ONLY:
        if tuple(argv[: len(allowed)]) == allowed:
            return argv
    return None


def rows(text):
    """(line index, row id, date, how, command or None, expect or None,
    fault or None) of every measure-after row."""
    out = []
    for i, line in enumerate(text.splitlines()):
        m = ROW_ID.match(line)
        if not m:
            continue
        st = status_of(line)
        ma = MEASURE_AFTER.match(st)
        if not ma:
            continue
        how = ma.group(2)
        cmds = BACKTICK.findall(how)
        fault = None
        cmd = None
        if len(cmds) != 1:
            fault = f"names {len(cmds)} backticked commands, not one"
        else:
            cmd = cmds[0]
            if read_only(cmd) is None:
                fault = "its command is not on the read-only list, so it is never run"
        ex = EXPECT.search(how)
        out.append((i, m.group(1), datetime.date.fromisoformat(ma.group(1)), how, cmd,
                    ex.group(1) if ex else None, fault))
    return out


def run(argv, homelab):
    if homelab:
        argv = [homelab] + argv[1:]
    try:
        p = subprocess.run(argv, capture_output=True, text=True, timeout=180, check=False)
    except (OSError, subprocess.TimeoutExpired) as e:
        return None, str(e)
    return p.returncode, (p.stdout or "") + (p.stderr or "")


def main(args):
    def opt(name):
        if name in args:
            i = args.index(name)
            if i + 1 < len(args):
                return args[i + 1]
        return None

    path = opt("--register") or REGISTER
    today_s = opt("--today")
    today = datetime.date.fromisoformat(today_s) if today_s else datetime.date.today()
    text = open(path, encoding="utf-8").read()
    all_rows = rows(text)
    due = [r for r in all_rows if r[2] <= today]
    shown = all_rows if "--all" in args else due
    if not shown:
        print(f"measure-due: no measure-after row is due on {today} "
              f"({len(all_rows)} waiting for a later day)")
        return 0
    lines = text.splitlines()
    verdicts = {"pass": 0, "fail": 0, "needs-eyes": 0}
    rewritten = 0
    for i, rid, date, how, cmd, expect, fault in shown:
        state = "due" if date <= today else f"waits until {date}"
        print(f"{rid} ({state}): {how[:160]}")
        if "--run" not in args or date > today:
            continue
        if fault:
            verdicts["needs-eyes"] += 1
            print(f"  needs-eyes: {fault}")
            continue
        code, out = run(read_only(cmd), opt("--homelab"))
        if code is None or code != 0:
            verdicts["fail"] += 1
            print(f"  fail: `{cmd}` did not answer ({out.strip()[-200:]})")
            continue
        if not expect:
            verdicts["needs-eyes"] += 1
            print(f"  needs-eyes: no `expect /…/` to judge by; the output ends:")
            print("    " + "\n    ".join(out.strip().splitlines()[-8:]))
            continue
        m = re.search(expect, out, re.M)
        if not m:
            verdicts["fail"] += 1
            print(f"  fail: /{expect}/ not in the output of `{cmd}`; it ends:")
            print("    " + "\n    ".join(out.strip().splitlines()[-8:]))
            continue
        verdicts["pass"] += 1
        seen = " ".join(m.group(0).split())[:120].replace("|", "\\|")
        now = datetime.datetime.now().strftime("%H:%M")
        done = f"done {today}: `{cmd}` read {seen!r} at {now} (scripts/measure-due.py)"
        print(f"  pass: {seen!r}")
        if "--write" in args:
            old = status_of(lines[i])
            lines[i] = lines[i][: lines[i].rindex(old)] + done + lines[i][lines[i].rindex(old) + len(old):]
            rewritten += 1
        else:
            print(f"  status to write: {done}")
    if "--run" in args:
        print(f"measure-due: {verdicts['pass']} pass, {verdicts['fail']} fail, "
              f"{verdicts['needs-eyes']} need eyes of {len(due)} due row(s)")
    if rewritten:
        with open(path, "w", encoding="utf-8") as f:
            f.write("\n".join(lines) + ("\n" if text.endswith("\n") else ""))
        print(f"measure-due: wrote {rewritten} done status(es) into {path}; commit them")
    unsettled = len(due) - rewritten
    if unsettled and ("--run" in args or "--check" in args):
        why = os.environ.get("UNMEASURED_OK", "").strip()
        print(f"RELEASE BLOCKED — {unsettled} due measure-after row(s) are not measured yet "
              f"(run `scripts/measure-due.py --run --write`, read what failed or needs eyes, "
              f"commit the register).", file=sys.stderr)
        if why:
            print(f"measure-due: going ahead anyway, UNMEASURED_OK: {why}", file=sys.stderr)
            return 0
        return 1
    return 0


if __name__ == "__main__":
    sys.exit(main(sys.argv[1:]))
