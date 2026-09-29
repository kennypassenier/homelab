# Development guide

Working on the homelab code itself: how a change gets from your editor to
the running host, and what physically stops a bad change on the way.

## 1 · First thing after cloning

```bash
make hooks
```

That runs `git config core.hooksPath .githooks`, which is what activates
the commit gates. **It cannot be automatic**: `core.hooksPath` is local
git configuration and is deliberately not carried inside a repository —
otherwise cloning a repo would let it run scripts on your machine. So
every clone, on every machine, needs this once.

Verify with:

```bash
git config core.hooksPath      # must print .githooks
```

If that prints nothing, there is no enforcement: commits are accepted with
failing tests and without traceable messages. This is not hypothetical.
Releases v3.0.1 through v3.1.1 were committed from a session opened in a
different directory, where the second layer below does not load, and
nothing blocked them. The gates were run by hand every time and were
green — but "someone remembered" is not a gate.

## 2 · The two layers

Both run the same script, `.claude/hooks/gates.sh`, so there is exactly one
definition of "the gates": `cargo fmt --check`, `cargo clippy --workspace
--all-targets -D warnings`, and `cargo test --workspace`.

| Layer | Lives in | Runs when | Covers |
|---|---|---|---|
| git-native | `.githooks/pre-commit`, `.githooks/commit-msg` | every `git commit` | any terminal, editor or session |
| session hook | `.claude/hooks/check-commit.sh` | Claude Code `git commit` | only a session opened in this directory |
| release | `make release` (`make check` by hand) | every release | the full gate plus `cargo deny` advisories, gitleaks over the whole history and the MSRV check; a failure refuses the release |

Layer 1 is the one that always holds. Layer 2 is a faster feedback loop
that catches the same thing earlier in an assisted session. There is no
GitHub Actions CI since 2026-09-29 (Kenny: every build and check runs
locally, GitHub only receives the result); the checks it ran moved into
layer 1 (gitleaks over the staged diff, `cargo deny` when `Cargo.lock`
moves) and into `make release` (the whole history, the MSRV).

Nor does anything stop `homelab deploy`/`apply` from sending files that
are in no commit: they deploy the working tree. Since fix-141 they warn
when the stack directory differs from `HEAD`, and the host records the
commit (plus "N uncommitted file(s)") in its intent history and in
`homelab status`; `homelab ping` names the build of both binaries.

`make check` runs what the four CI jobs ran until 2026-09-29: the gate,
`advisories` (`cargo deny check advisories` against what `Cargo.lock`
pins), `secrets` (gitleaks over the whole history, redacted; false positives
are allowlisted per file in `.gitleaks.toml`) and `msrv` (`cargo check` with
the `rust-version` from `Cargo.toml`). Both scanners are pinned release
binaries, checksum-verified and cached in `~/.cache/homelab-scanners`, so
WSL and Garuda need no package for them. `make release` refuses when any of
them fails.

## 3 · What the gates block

**A failing build, lint or test.** Warnings count as errors — a clippy
warning is a failed commit, not a note for later.

**A message without traceability.** Every commit message names the feature
IDs it implements, in brackets: `feat(e8): zfs replication [E8, AR3]`.
Pure infrastructure commits (hooks, CI, tooling) use `[meta]`. The IDs come
from `docs/FEATURES.md` (features) and `docs/ARCHITECTURE_DECISIONS.md`
(AR-numbers); they are permanent, which is what makes it possible to ask
years later why a line of code exists.

Merge, revert, fixup and squash messages are exempt — git generates those
itself and they carry no IDs of their own.

**Bypassing**, when you genuinely need to (committing from a machine
without a Rust toolchain, for instance):

```bash
git commit --no-verify
```

That is a deliberate act, visible in your shell history. CI still runs the
gates on push, so a bypassed commit does not get to hide.

## 4 · The everyday commands

```bash
make gate                    # the hooks' gates.sh with the whole suite; stamps .git/gate-pass
make test                    # tests only
make build                   # debug build of the workspace
make host-binary             # release build of the host daemon for Debian 12
```

## 5 · Releasing

```bash
make release VERSION=3.2.0
```

Runs the full gate locally, stamps the workspace version, commits, tags
`v3.2.0` and pushes. It refuses on a dirty working tree or an existing tag.
The gate runs once per release: a green `make gate` on a clean checkout
stamps its tree and toolchain (`rustc -V`, `node -v`) in `.git/gate-pass`
through the shared `gate-stamp` helper (`.githooks/gate-stamp` finds it in
the workstation repository), and `make release` skips its own gate when the
clean HEAD tree and toolchain equal that stamp. The version-bump commit
skips the pre-commit gates when every changed line is an x.y.z `version =`
line in `Cargo.toml` or `Cargo.lock`. Without the helper both skips are off;
the commit-msg ID check still runs.
GitHub then re-runs the gate — a red gate blocks the release — and
publishes `homelab-host`, `homelab` and `SHA256SUMS` as a GitHub Release.

Semver here: breaking or architectural change = major, new feature = minor,
fix = patch.

**Publishing changes nothing on the running host.** Rolling out is a
separate, deliberate act:

```bash
homelab release-update       # or press u in the TUI when the badge appears
```

The client downloads the release, verifies its checksum against
`SHA256SUMS`, and ships the binary over the line into the host's
self-update pipeline: selfcheck, keep the previous binary, armed rollback,
restart. A release that crashes on start is rolled back automatically by
systemd.

Emergency path without GitHub: `make host-binary` then
`homelab self-update target-debian/release/homelab-host`.

## 6 · House rules that shape the code

These are the project-wide rules from `~/Projects/dev-procedure/`, in the
form they take here:

- **Every live-found bug becomes a test before the fix.** "Before" means
  the test demonstrably fails first; the same commit is fine, the order of
  work is what counts.
- **Tests use real dependencies where possible** (real git, real files,
  real subprocesses). Mocks are for what cannot be real: the clock,
  network failures, Proxmox itself.
- **Secrets never reach git, argv, logs, test fixtures or backups**, and
  the test suite asserts it — see the plaintext scans in
  `core/tests/secrets_tests.rs`.
- **Every error message carries a remedy** in the message itself.
- **The no-touch list in `core/src/safety.rs` is law**: no operation may
  touch those VMs and containers, and a property test walks every
  mutating operation against every no-touch id.
- **Writes are atomic**, defaults fail closed, and nothing is silently
  capped, truncated or skipped.
