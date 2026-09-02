# Latch key loss, 2026-09-02 — what each project lost, and how to recover

*Hand this to the session of the project it concerns. Written from the
homelab (`stacks`) project, which hit this first and recovered from it.*

## What happened

At 11:41 a full system upgrade ran on Kenny's workstation (dozens of
packages). One minute later the KDE wallet file was rewritten and the entire
`Secret Service` folder was gone — with it the latch keys, the GitHub
tokens, the copilot token, IntelliJ and zed. `kwallet` itself was **not**
updated that day; the last version bump was 2026-08-27. The exact mechanism
is unproven and nobody should invent one. What is established: a daemon
holding the only copy of every key, through an in-place library swap of that
size, is a hazard Kenny's own notes already warn about.

The lesson, stated plainly: **a keyring protects against being READ, not
against being LOST.** Confidentiality is not durability.

## What was actually lost — measured, not assumed

`latch verify` across the whole repository, 2026-09-02:

| | files | meaning |
|---|---|---|
| `ok` — **stacks** | 13 | recovered, fully readable |
| `format` — ~~**homelab**~~ | ~~13~~ | removed from the repo 2026-09-02, form P1 |
| `format` — **latch-rs** | 10 | latch v1 format |
| `no-key` — **almanac** | 1 | genuinely key-locked |
| `no-key` — **hub-clients** | 1 | genuinely key-locked |

**Read that table carefully before panicking.** Only **two files** in the
whole repository are unreadable *because of the key loss*. The other 23 were
already unreadable before it: they are in latch v1 format, which latch v2
cannot open with any key. Their state did not change on 2026-09-02.

Per project:

- **almanac** — one file, `almanac/dev/.env.enc`. A `dev` environment. The
  live service runs on CT 112 and reads
  `/appdata/almanac/almanac-config/`, which the almanac stack's own restic
  backup covers. Nothing operational depends on this file.
- **hub-clients** (linked to `~/Projects/newsflash`) — one file,
  `hub-clients/dev/.env.enc`. **This one was production, and this document
  said otherwise.** Corrected by the newsflash session on 2026-09-02: that
  project only ever had ONE environment, which happens to be named `dev`, and
  it carried the `KYU_TOKEN` its live systemd unit injects. The unit
  crash-looped from 11:42, hit `StartLimitBurst=5` at 11:44:30 and stayed
  stopped for roughly two hours until that session recovered it. Four queued
  messages rendered on restart; none were lost.

  The mistake worth carrying: I read the environment NAME and concluded
  "dev, therefore disposable", instead of asking what consumed the file. An
  environment is called whatever somebody typed once. **Before deciding a
  latch file is disposable, find its consumer** — `systemctl show <unit> -p
  EnvironmentFiles` on whatever machine actually runs it. Note *whatever
  machine*: a sweep of the Proxmox containers would not have found this one,
  because newsflash runs on the workstation.
- **homelab** — REMOVED on 2026-09-02 (Kenny, form P1). It held 13 files,
  all `dev`, all describing the **v1 architecture**: `stacks/gateway/nginx-proxy-manager`, `stacks/todo/vikunja`,
  `stacks/cloudflared`, plus a `stacks-backup/` tree. NPM was replaced by
  Traefik, `todo` no longer exists. This is a historical archive of a system
  that has been gone for months, and it was already format-locked.
- **latch-rs** — 10 files, every one `tests.fixtures.*`. Test fixtures for
  latch's own suite, not secrets.

So: one running service DID lose a credential it needed (hub-clients, above),
and it was down for two hours before anyone knew. The rest is archives. The
first version of this paragraph said no service was affected, which was an
inference from environment names rather than a measurement of consumers.

## How the homelab recovered (the recipe that worked)

1. **Kenny ran `latch login`** — the PAT is his to enter, nobody else's.
2. **Back up the ciphertext first.** `cp -a ~/.latch/repo ~/.latch/repo.backup-<date>`.
   Re-minting is one-way; this makes it reversible.
3. **Check git actually ignores `.env`** before writing any plaintext into a
   working tree: `git check-ignore -v <path>/.env`. Do not skip this.
4. **Collect the plaintext from wherever it still runs.** For the homelab
   that was the host vault (`/var/lib/homelab/secrets/<stack>/<app>.env`),
   which the deploy has always written. For a native service it is its
   `EnvironmentFile`. Place them where latch expects them.
5. **`latch commit --env <env>`** — this mints the new project key.
   ⚠ It publishes **only what is on disk**: anything you did not place is
   REMOVED from that environment. Check the removal list before pushing.
   The homelab's commit removed seven entries, each verified dead first.
6. **Verify BEFORE pushing**: `latch cat <file> --env <env>` and compare
   against the source you collected.
7. **`latch push`**, then shred the plaintext out of the working tree.
8. **`latch key backup <file>`** immediately, with a passphrase that is not
   in any keyring, and put that file somewhere that is not this machine.

## What you lose by re-minting

The current values survive; the **history** does not. `latch history` and
`latch rollback` start again from generation 1. If a project's older secret
versions matter to it, say so before step 5 rather than after.

## The state to leave behind

After recovery, `latch key backup` again and make sure the escrow lands
somewhere that is not the machine that lost it. The homelab's now sits in
three places of different kinds: the workstation keyring, the Proxmox host's
vault, and inside restic on Google Drive.
