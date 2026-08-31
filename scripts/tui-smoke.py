"""Drive the TUI in a real pty long enough to see whether it draws.

    python3 scripts/tui-smoke.py ./target/release/homelab tui --offline

Written 2026-08-31 to check that a ratatui major upgrade had not broken the
interface, and kept because it answers a question Kenny asked earlier that
day: whether the TUI can be exercised without him sitting in front of it.

It can, up to a point. This proves the TUI starts, draws, and does not
panic, and it shows the text that reached the screen — enough to catch a
library upgrade that breaks rendering, a panic on startup, or a tab that
stopped being drawn. It does NOT interpret the ANSI stream, so it cannot
tell you the layout is right or that a colour is wrong. Those still need
eyes.

The pty matters. crossterm asks the terminal where the cursor is (ESC[6n)
and refuses to start if nothing answers within a moment. `script -qec`
gives a pty that never answers, so the TUI exits with "the cursor position
could not be read" — which looks exactly like a broken build and is not
one. This harness answers the query, which is the whole reason it works
where the obvious approach does not.
"""
import os, pty, select, signal, sys, time, fcntl, termios, struct

cmd = sys.argv[1:]
pid, fd = pty.fork()
if pid == 0:
    os.execvp(cmd[0], cmd)

fcntl.ioctl(fd, termios.TIOCSWINSZ, struct.pack("HHHH", 40, 120, 0, 0))
out = bytearray()
deadline = time.time() + 8
answered = 0
while time.time() < deadline:
    r, _, _ = select.select([fd], [], [], 0.3)
    if not r:
        continue
    try:
        chunk = os.read(fd, 65536)
    except OSError:
        break
    if not chunk:
        break
    out += chunk
    if b"\x1b[6n" in chunk:
        os.write(fd, b"\x1b[1;1R")
        answered += 1

os.kill(pid, signal.SIGTERM)
time.sleep(0.5)
try:
    os.waitpid(pid, os.WNOHANG)
except ChildProcessError:
    pass

text = out.decode("utf-8", "replace")
import re
plain = re.sub(r"\x1b\[[0-9;?]*[a-zA-Z]", "", text).replace("\x1b", "")
print(f"cursorvragen beantwoord: {answered}")
print(f"bytes ontvangen:        {len(out)}")
print(f"paniek in de uitvoer:   {'JA' if 'panicked' in text else 'nee'}")
words = [w for w in re.findall(r"[A-Za-z][A-Za-z0-9_.-]{3,}", plain)]
seen, uniq = set(), []
for w in words:
    if w.lower() not in seen:
        seen.add(w.lower()); uniq.append(w)
print("herkenbare tekst op het scherm:")
print("  " + " ".join(uniq[:40]))
