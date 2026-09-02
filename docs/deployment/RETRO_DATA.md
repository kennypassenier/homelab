# Numbers for the retrospective

*T76. Kenny asked for a report at the retrospective on how many rounds this
project took and whatever else the numbers say, and flagged it as a
candidate to become a permanent step in the procedure.*

**Why this file exists rather than a promise to count later.** Some of it is
reconstructable afterwards and some is not. Git history, the register's own
dated rows and the test count all survive; how many forms were sent, how
many items Kenny bounced, and how many decisions arrived as prose instead of
a form live only in the transcript — and a transcript is what a context
compaction eats. So the counts are taken as the work runs, not promised.

Update this file at the end of each working session. It is data, not
analysis: the reading of it belongs in the retrospective.

## Session of 2026-09-01 evening → 2026-09-02 early morning

Measured 2026-09-02 04:05 from git and the register, not from memory.

| | |
|---|---|
| Commits | 63 |
| Lines changed | +8378 / −1616 |
| Tests | 311 (workspace, all green) |
| Register rows | 183 findings, 75 tasks, 97 decisions, 11 pending measurements |
| Findings raised this session (F160–F183) | 24 |
| — of which fixed the same session | 18 |
| — still open | 4 |
| — closed by Kenny's decision without a fix | 1 |
| — parked into a task | 1 |
| Measurements still open | 7 |

### Forms

Counted from the conversation, because they exist nowhere else.

| Form | Items | Outcome |
|---|---|---|
| X · openstaande beslissingen | 9 | all answered; 2 "Eigen antwoord" |
| Y · parallel werken (first) | 4 | bounced — the options did not say what they did |
| Y · parallel werken (second) | 3 | bounced again on the same ground (D82) |
| D82 · correctieform + Y1/Y4 | 11 | ratified, one amendment |
| Z1–Z5 · registry cache + drill findings | 5 | all answered |
| Z6–Z9 + C1–C9 · photos, host, drill, lenses, correction | 13 | all answered |
| Z10 · kyu backup source | 1 | answered |
| Z11–Z13 · the CT 116 rule | 3 | answered |

Eight forms, 49 items. **Two were bounced on the same defect** — options
that named a choice without saying what it would do — which produced
standing rule D82 and its measurement M-D82.

### Corrections Claude had to make to its own work

Recorded because the count matters more than any single one.

1. Four TRaSH `trash_id` hashes written from memory (T7).
2. "No cleanup policy of any kind" for the registry — there was a 168h
   default, and the first fix would have made the cache live 4× longer.
3. `Firewall: Apply` named as a required OPNsense privilege; it does not
   exist in that version.
4. Norm "N3" written when standing rule 28 already said it — the first
   reading of M-R30, and it failed.
5. "Fixed, verified here" for the kyu backup, on evidence that could not
   tell a nightly run from a hand-started one.
6. Six sabotage attempts before one honest test for T69, each failing a
   different way.

### What the numbers already suggest

Left deliberately thin — the reading belongs in the retrospective, not here.

- Of 24 findings, **at least 4 could only be found by FOLLOWING a document
  or drill rather than reading code**: the wrong disk count, the replica on
  a live path, the host config in no backup, and the missing host-restore
  layer.
- The dominant fault shape is unchanged from earlier in the project: a
  mechanism that runs, reports success, and is wired to nothing.
