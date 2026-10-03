#!/usr/bin/env python3
"""Outer watchdog for the whole-screen run (redesign-integrate-7).

A per-test deadline lives inside the test runner and cannot fire when the
browser or its driver connection itself hangs, or when the runner's event
loop is blocked (2026-10-03: two runs stood still for 12 and 17 minutes).
This watches from outside: every TICK seconds it reads the runner's
progress log (node --test's TAP stream: a `# Subtest:` line as a case
starts, `ok` / `not ok` as it ends). When no progress line came for IDLE
seconds AND the run's headless browser processes used less than CPU
seconds of processor time in that window, the run is stuck, not slow: the
watchdog kills the run's whole process tree, prints

    watchdog: no progress for 90 s; last test started: <name>

writes the same line to <log>.fired and exits 3. It exits 0 as soon as the
run ends on its own.

usage: e2e-watchdog.py <run pid> <progress log>
env:   WATCHDOG_TICK_S (15), WATCHDOG_IDLE_S (90), WATCHDOG_CPU_S (1.0)
"""
import os
import re
import signal
import sys
import time

TICK = float(os.environ.get("WATCHDOG_TICK_S", "15"))
IDLE = float(os.environ.get("WATCHDOG_IDLE_S", "90"))
CPU = float(os.environ.get("WATCHDOG_CPU_S", "1.0"))
HZ = os.sysconf("SC_CLK_TCK")
BROWSER = re.compile(r"chrom|headless", re.I)


def procs():
    """pid -> (ppid, comm, cpu seconds) for every process we can read."""
    out = {}
    for d in os.listdir("/proc"):
        if not d.isdigit():
            continue
        try:
            with open(f"/proc/{d}/stat") as f:
                s = f.read()
        except OSError:
            continue
        # comm is in parentheses and may hold spaces: split after the last ')'.
        comm = s[s.index("(") + 1 : s.rindex(")")]
        rest = s[s.rindex(")") + 2 :].split()
        if rest[0] == "Z":
            continue  # ended, not yet reaped by its parent: gone
        ppid = int(rest[1])
        cpu = (int(rest[11]) + int(rest[12])) / HZ
        out[int(d)] = (ppid, comm, cpu)
    return out


def tree(root, table):
    """The pids under `root` (root included), children before parents."""
    kids = {}
    for pid, (ppid, _, _) in table.items():
        kids.setdefault(ppid, []).append(pid)
    order, stack = [], [root]
    while stack:
        p = stack.pop()
        order.append(p)
        stack.extend(kids.get(p, []))
    return list(reversed(order))


def browser_cpu(root, table):
    return sum(
        table[p][2]
        for p in tree(root, table)
        if p in table and BROWSER.search(table[p][1])
    )


def last_started(path):
    """The last case started: the harness's own line in <log>.started (written
    before the case's body runs, so it is there even when the case blocks its
    process before the runner reports it), else the TAP stream's."""
    for src, pat in ((path + ".started", r"^started: (.*)$"),
                     (path, r"^\s*# Subtest: (.*)$")):
        try:
            with open(src, errors="replace") as f:
                names = re.findall(pat, f.read(), re.M)
        except OSError:
            continue
        if names:
            return names[-1]
    return "none"


def size(path):
    total = 0
    for p in (path, path + ".started"):
        try:
            total += os.path.getsize(p)
        except OSError:
            pass
    return total


def main():
    run, log = int(sys.argv[1]), sys.argv[2]
    seen = size(log)
    since = time.monotonic()
    table = procs()
    cpu0 = browser_cpu(run, table)
    while True:
        time.sleep(TICK)
        table = procs()
        if run not in table:
            return 0
        now = size(log)
        cpu = browser_cpu(run, table)
        if now != seen:
            seen, since, cpu0 = now, time.monotonic(), cpu
            continue
        idle = time.monotonic() - since
        if idle >= IDLE and cpu - cpu0 < CPU:
            line = (
                f"watchdog: no progress for {int(idle)} s; "
                f"last test started: {last_started(log)}"
            )
            print(line, flush=True)
            print(line, file=sys.stderr, flush=True)
            for p in tree(run, table):
                try:
                    os.kill(p, signal.SIGKILL)
                except OSError:
                    pass
            with open(log + ".fired", "w") as f:
                f.write(line + "\n")
            return 3
        if idle >= IDLE:
            # Busy, not stuck: the browser is working. Start a new window.
            since, cpu0 = time.monotonic(), cpu


if __name__ == "__main__":
    sys.exit(main())
