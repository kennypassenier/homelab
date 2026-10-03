# Session guards — proposal for the coordinator

Kenny, 2026-10-03: "je zou je gedragen als een world-class developer, maar
ik zie vaak amateuristische fouten, die moeten eruit". The faults that live
in this repository now have guards here (`fix-guards-1` to `fix-guards-8` in
`REGISTER.md`). The ones below come from how a Claude session works, not
from the repository, so nothing in this repository can catch them. Each one
names the hook that would. None of this is built: every item changes
`~/.claude/settings.json`, `~/Projects/dev-procedure` or a global hook, and
those are **global changes, which need Kenny's explicit go** (standing rule
on global vs project procedure changes).

Claude Code hooks referred to: `PreToolUse` (runs before a tool call and can
refuse it with a reason Claude reads) and `Stop` (runs when a turn ends and
can send Claude back with a reason).

| # | Pattern (how often it came up) | Hook that would catch it | Where it would live |
|---|---|---|---|
| session-1 | **Wrong or stale times in progress updates**: an `[HH:MM]` that is not the local time when the message is sent (guessed, carried over from an earlier update, or UTC written as local). Kenny's standing five-minute-update rule depends on the time being true. | `PreToolUse` on the reply tool (`mcp__hearthbot__reply`, and `SendMessage`): every `[HH:MM]` in the text must be within 2 minutes of `date +%H:%M` in the machine's zone; otherwise refuse with the real time. | global hook script, `~/.claude/settings.json` |
| session-2 | **Updates that say "still busy" without what moved** (rule of 2026-09-30). | Same reply hook: a message that opens with `[HH:MM]` must name what finished, what is running and what is left (three labelled parts); refuse a bare status line. | same hook |
| session-3 | **Bare internal codes in text to Kenny** (`fix-240`, `feat-shell-1`) with no one-line gloss — the rule repeated most often in `~/.claude/CLAUDE.md` §3. The dashboard and CLI half is now `fix-guards-8` here. | Reply hook (and the form builder's `post_widget` input): every register id (the kinds in `.githooks/register-id-kinds.txt`, the list this repository's own checks read: `fix-240`, `feat-shell-1`, `H8` — a generic `[a-z]+(-[a-z]+)*-\d+` would also flag `sha-256` and `utf-8`) must be followed on the same or next line by a `↳` gloss; refuse otherwise, listing the ids. | same hook |
| session-4 | **Claims without a live measurement** ("the dashboard runs 3.70.0", CORRECTIONS 2026-10-02; fix-15 "never ran" while it ran nightly; fix-6, fix-13). The register half is `fix-guards-2` here. | Reply hook: a sentence claiming a running version or a live state ("runs/draait/live on X.Y.Z", "is fixed", "works") must carry the measurement it rests on in the same message — a command in backticks with its output, or the word "measured/gemeten" with a time. Flag, do not rewrite. | same hook |
| session-5 | **Guessing instead of looking up** — Live view control names, API shapes (fix-209 `?fresh=1` against a plain bool, fix-257 a map where the host sends a list, fix-22 `kyu --check` assumed read-only, fix-43 `--version` assumed harmless). The product half exists: an unknown `homelab ui click` name is refused with the controls on screen (fix-239). | `PreToolUse` on Bash for `homelab ui click|open|press <name>`: run `homelab ui state --json` first and refuse a name the tab does not declare, quoting the declared ones — one round trip instead of a refused step in Kenny's tab. For an external API: `PreToolUse` on Write/Edit of a new client type refuses until a captured real response for that endpoint exists in the session scratchpad (the "test API calls first" rule as a gate). | project hook `.claude/hooks/` (Live view part); global hook (API part) |
| session-6 | **An irreversible step run after Kenny said stop** (CORRECTIONS 2026-10-02: uptime destroyed twelve seconds after "fix eerst de problemen"). | `PreToolUse` on Bash for `homelab destroy|wipe|rebuild`, `ui press confirm` on a destructive dialog, and `pct destroy`: read the session's own transcript (a hook receives its path as `transcript_path` in its JSON input) and refuse when a message from Kenny arrived after the step was requested, quoting it. A shell hook cannot call an MCP tool such as `mcp__hearthbot__fetch_thread`; a message that has not reached the transcript yet is out of its reach, so this narrows the window rather than closing it. | global hook |
| session-7 | **A decision forwarded between sessions read as a form to build** (CORRECTIONS B8; F279). | `PreToolUse` on `SendMessage` / `send_message`: a message carrying an option list ("Optie", "(Aanbevolen)", a bulleted choice) must open with `RENDERS: <session>`; refuse otherwise. The fallback the ratified correction itself names. | global hook |
| session-8 | **The ISO-time gate does not read `.js`**: `.githooks/check-timestamps.sh` (shared, `HOOK_VERSION=4`, synced from dev-procedure) lists `html, tsx, jsx, vue, …` but not plain `.js`, and this dashboard is plain ES modules. This repository now has its own check (`admin/web/test/user_text.test.js`); the shared hook should add `js` (with the `timestamp-ok` escape it already has) so every project with a plain-JS front end is covered. | edit `ui_files` in dev-procedure's `hooks/check-timestamps.sh`, then `sync-hooks.sh` | `~/Projects/dev-procedure` (global) |

## What the repository already enforces since `fix-guards-*`

For completeness, so the coordinator does not build these twice:

- a register row naming a test that does not exist (`.githooks/check-register.py`
  at commit, `register_guard_tests.rs` over the tree);
- a status a commit writes that claims more than holds
  (`.githooks/check-register.py`, commit): done without `done <date>:
  measured …`, released without `measure-after <date>: <how>`, obsolete or
  superseded without a dated reason, open without `open: <who> <date>,
  <why>`, a date in the future; a new row whose tests are not said to have
  failed first;
- a release on top of an earlier release's unmeasured rows or an overdue
  `measure-after` (`make release`; an override lands in the tag's
  annotation);
- a release while `config/host.toml` and the host disagree (`homelab host
  diff`, `make release`);
- doubled, gapped or split INVARIANTS numbering (`.githooks/check-register.py`
  at commit and over the tree) and stale citations of a row
  (`invariants_doc_tests.rs`);
- register ids in text a person reads (dashboard: `user_text.test.js`; CLI
  and host messages: `user_text_ids_tests.rs`; added strings at commit:
  `.githooks/check-register.py`) and ISO times in the dashboard's text
  (`user_text.test.js` only — the Rust check does not look for times);
- a tool, not a gate: `make fail-first` runs a range's new `covers:` tests
  against the code before it and reports which fail there. Nothing runs it
  automatically; the commit gate only checks that a new row SAYS its tests
  failed first.
