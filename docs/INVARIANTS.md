# Invariants — what Kenny has said must always be so

Kenny, 2026-10-02: fixed things kept coming back (the step counter three
times, Pause/Stop gone twice, a Live view version mismatch) — not because
the fixes were wrong, but because nothing stood guard once they landed.
This is the one list of everything he has stated as "must always be so"
for the admin dashboard: one row per invariant, the one-sentence rule,
where he said it, and the test(s) that pin it. `core/tests/invariants_doc_tests.rs`
refuses a row whose test does not exist, the same way `register_tests.rs`
refuses a register row whose test does not exist.

A row's test column cites either a Rust function (`` `fn_name` ``, found by
`fn fn_name(` anywhere in the workspace) or a JS one
(`` `file.test.js: "the test's own description"` ``, found as that exact
string inside that exact file). A test still listed here with `(todo)`
exists and is wired, but is intentionally not yet enforcing (Node's
`test(..., { todo: "reason" }, fn)` or Rust's `#[ignore = "reason"]`) — the
reason is the thing waiting on it, never "not written yet".

| # | Invariant | Kenny said (date · source) | Test(s) |
|---|---|---|---|
| 1 | Pause and Stop are visible in every Live view drive state (a job already running finishes on its own; Pause only ever holds what has not started yet, honestly said). | 2026-10-02 06:55, `docs/deployment/REGISTER.md` fix-172/fix-173; `docs/admin/DECISIONS.md` "Live view: announce, plan and pause" (2026-09-29, live-control). | `fix_172_pause_holds_a_batch_at_the_boundary_before_its_next_job`, `fix_172_a_queue_without_a_pause_gate_never_waits` (`admin/tests/act_queue_tests.rs`); `admin/web/test/announce.test.js: "fix_173_pause_says_honestly_that_a_running_job_finishes_on_its_own"`, `admin/web/test/announce.test.js: "fix_173_pause_on_a_driven_batch_names_which_stack_still_runs"`; `admin/web/test-e2e/invariants.e2e.js: "invariants: Pause and Stop are visible in every Live view drive state"` (Playwright, demo host, the buttons as Kenny actually sees them). |
| 2 | The step total is fixed from step 1 and equals the steps that actually run, for every action, including nested ones and an Apply/batch of several — never a number that climbs with the run. | 2026-10-02 06:55, `docs/deployment/REGISTER.md` fix-171 (three rounds: deploy/destroy/install-native/update, then every remaining stepping op, both from the same incident). | `core/tests/step_plan_tests.rs: deploy_announces_its_plan_before_any_step`, `deploy_plan_length_is_independent_of_app_count`, `deploy_a_run_with_no_firewall_or_log_shipper_skips_those_steps_and_still_reaches_m`, `destroy_announces_its_fixed_plan_and_every_step_runs`, `install_native_announces_its_fixed_plan_before_the_first_step`, `update_skips_the_rest_of_an_apps_steps_when_its_policy_gate_says_skip`; `admin/src/core/actions_progress.rs: m_comes_from_the_announced_plan_not_from_a_past_runs_count`, `m_is_none_until_a_plan_arrives`, `a_skipped_step_still_advances_n_to_meet_the_announced_total`, `set_total_fixes_a_multi_command_jobs_total_before_the_first_mark`; the round-3 `fix_171_*` tests in `core/tests/native_tests.rs`, `native_backup_tests.rs`, `native_restore_tests.rs`, `native_rollback_tests.rs`, `native_update_check_tests.rs`, `m4_ops_tests.rs`, `devicebackup_tests.rs`, `homeaddress_tests.rs`, `secondcopy_tests.rs`, `zfs_tests.rs`, `declarative_cleanup_tests.rs`, `fix_70_template_tests.rs`, `restore_runs_tests.rs`; `core/src/runner.rs`'s own `debug_assert!`-firing unit tests (plan-before-mark). |
| 3 | The progress bar's percentage equals the step counter's N/M whenever a step total exists (e.g. 263/310 reads 85%, never 99%); a time-based estimate only ever feeds "Expected remaining", never the bar itself. | 2026-10-02 11:02, Kenny: "Die twee moeten samen lopen." | `admin/web/test/jobs_percent_sync.test.js: "invariants: the percentage equals N/M when a step total exists, never a higher time-based guess"` **(todo: waits for `jobs.js percent()` to prefer the step fraction over the time-based one when `p.m` is set — another helper's fix, in flight as of this row)**; the unconstrained case is pinned green today by `admin/web/test/jobs_percent_sync.test.js: "invariants: with no step total, the time-based estimate is still allowed to drive the bar"`. |
| 4 | A Live view step sent to a tab that is on a different dashboard version is refused, naming both versions and `homelab ui reload`, rather than acted on against a page that cannot understand it. | 2026-10-02 07:07 (promised after "a morning of avoidable failures during the 3.70.0 rollout"), `docs/deployment/REGISTER.md` fix-185. | `admin/tests/follow_live_tests.rs: fix_185_a_step_is_refused_when_the_tab_runs_a_different_version`, `fix_185_reload_waits_for_the_tab_to_report_the_new_version`, `fix_185_a_stale_stop_never_reaches_into_a_later_round`; `admin/tests/follow_tests.rs: fix_185_reads_repo_names_the_actions_that_read_the_repository`; `admin/src/core/drivelive.rs: fix_185_version_match_exempts_state_and_reload_and_names_the_fix`; `client/src/ui_preflight.rs: follow_unpushed_commits_name_them_and_say_push_first`, `follow_a_validation_error_names_the_stack_and_points_at_apply_plan`, `follow_stacks_involved_is_every_declared_stack_for_apply_and_the_form_list_otherwise`. |
| 5 | Every UI, check and tile string a person reads is English; Dutch is reserved for the parents' dashboards, which are not in this repository. | 2026-10-02, `docs/deployment/REGISTER.md` fix-178. | `checks_yml_text_has_no_dutch_function_words` (`client/tests/tui_snapshot_tests.rs`). |
| 6 | Pages use grid structures: elements keep the same horizontal place and width across loading, empty, error and filled states (vertical growth from genuinely new content, e.g. a log, is not a violation — sideways movement or a resize is). | 2026-10-02, `CLAUDE.md` rule 6 ("Web pages use grid structures"); demonstrated by fix-177's pixel-identical skeleton/final backup-calendar grid and feat-ops-6's kv-grid job panel. | `admin/web/test-e2e/invariants.e2e.js: "invariants: the backup calendar shows its skeleton grid immediately, before any stack has answered"`; `admin/web/test-e2e/invariants.e2e.js: "invariants: the job dialog's panel does not shift sideways between its running and done states"`. |
| 7 | The dashboard's nav bar stays in one row (never folds into the hamburger) at desktop widths, with the brand reading "Homelab" (never the binary name) and the host version sitting beside the "Go to…" shortcut (never hundreds of pixels from it). | 2026-10-02, `docs/deployment/REGISTER.md` fix-176 (chassis-rs 3.1.0 nav rewrite); checked by hand at 1280/1440/1600/1920/3840 at the time, never committed as a running test until this row. | `admin/web/test-e2e/invariants.e2e.js: "invariants: the nav bar stays inline, with brand Homelab and the version beside Go to…, at 1280/1920/2560 CSS px"`. |
| 8 | The backup calendar shows its full-size grid, pulsing, at once — never a blank page while it loads. | 2026-10-02, `docs/deployment/REGISTER.md` fix-177. | `admin/web/test-e2e/invariants.e2e.js: "invariants: the backup calendar shows its skeleton grid immediately, before any stack has answered"` (same case as row 6: the loading state IS the skeleton grid). |
| 9 | A failed per-stack read is shown with its own reason, named, next to the stacks that did answer — never folded into "none" or left silently blank. | 2026-10-02, `docs/deployment/REGISTER.md` fix-179 (five unfinished error/empty states), fix-177 (the backup calendar's own per-stack error list). | `admin/web/test/perstack.test.js: "stackReadProgress: a page's own successful status counts as loaded, failed is named"`, `"stackReadProgress: every stack settled is done, and failures sort by name"`; `admin/web/test/reports.test.js: "fix-179: an unrecognised status with an HTML body names the edge/proxy, not the dashboard"`, `"fix-179: an unrecognised status with a plain-text body reads its first meaningful line"`, `"fix-179: an unrecognised status with no body at all still says the status, not nothing"`. |
| 10 | No app- or stack-specific knowledge lives in `core/`, `host/`, `client/` or `admin/` source: everything about a given app lives in its stack file. | Standing architecture bar (`CLAUDE.md` / `~/Projects/dev-procedure/STANDING_RULES.md`: "declarative, generic, no special cases"); enforced since before this round. | `app_knowledge_no_declared_app_or_stack_is_named_in_the_code` (`core/tests/app_knowledge_guard_tests.rs`). |
| 11 | Only urgent events are pushed to phone, desktop and the office lights; everything else lands in the dashboard's notification centre, with history, and nowhere else. | 2026-09-30, `docs/admin/DECISIONS.md` "Notifications and Grafana" (notify-routing, push-edge). | `core/tests/notify_routing_tests.rs: notify_routing_a_failed_backup_update_or_deploy_is_urgent`, `notify_routing_everything_else_goes_to_the_centre_only`, `notify_routing_the_urgent_alerts_are_an_explicit_list`; `admin/tests/notify_shell_tests.rs: notify_routing_the_dashboard_pushes_only_the_urgent`; `admin/tests/act_notify_tests.rs: feat_ops_8_only_urgent_work_pushes_from_the_dashboard`. |

## How this list is kept honest

- **`core/tests/invariants_doc_tests.rs`** parses this table and fails if a
  row's `` `name` `` or `` `file: "description"` `` citation does not
  exist in the workspace's own sources — the same shape as
  `core/tests/register_tests.rs`'s `covers:` markers, applied to this
  document instead of the register.
- **`make invariants`** runs `admin/web/test-e2e/invariants.e2e.js`
  (`scripts/invariants-run.sh`: builds `homelab-admin --features
  demo-host`, starts it with a throwaway config and `HOMELAB_ADMIN_DEMO_HOST=1`,
  runs the suite, always tears the server down after, even on failure).
- It is wired into `.githooks/gate-carry.sh`'s `invariants` family and runs
  from `make gate`/`make release` (`GATE_FULL=1`) whenever `admin/web` or
  `admin/src` changed since its last recorded run, and always on a forced
  full run (`make gate-full`, `GATE_CARRY_MODE=full`) — never at commit
  time (standing rule 7's commit subset stays as it was; this suite needs a
  built binary and browsers, which a commit cannot afford).
- A new "must always be so" rule from Kenny gets a row here, in the same
  commit as its fix and its test — not added later from memory.
