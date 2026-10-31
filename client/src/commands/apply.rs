//! `homelab apply`: bring the host in line with the stacks directory.

use super::*;
use homelab_proto::{DeploySpec, FleetState};
use std::collections::BTreeMap;

pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "apply" => cmd_apply(c).await,
        _ => return false,
    }
    true
}

/// The flags and the stacks directory one `apply` was given.
struct ApplyFlags {
    base: String,
    skip_backup: bool,
    dry_run: bool,
    assume_yes: bool,
    plan_only: bool,
    force: bool,
}

impl ApplyFlags {
    fn parse(args: &[String]) -> Self {
        let has = |f: &str| args.iter().any(|a| a == f);
        ApplyFlags {
            base: args
                .get(2)
                .filter(|a| !a.starts_with("--"))
                .cloned()
                .unwrap_or_else(|| stacks_base().display().to_string()),
            skip_backup: has("--no-backup"),
            // fix-100 (see REGISTER.md)
            dry_run: has("--dry-run"),
            assume_yes: has("--yes"),
            plan_only: has("--plan"),
            force: has("--force"),
        }
    }
}

/// Asks on stderr unless the flags already decided; exits on a dry run or a
/// refusal.
fn confirm_plan(flags: &ApplyFlags, plan: &homelab_client::apply::ApplyPlan) {
    let answer = if flags.dry_run || flags.assume_yes || plan.deploy.is_empty() {
        None
    } else {
        use std::io::Write as _;
        eprint!("Deploy these {} stack(s)? [y/N] ", plan.deploy.len());
        std::io::stderr().flush().ok();
        let mut line = String::new();
        std::io::stdin()
            .read_line(&mut line)
            .ok()
            .filter(|n| *n > 0)
            .map(|_| line)
    };
    match homelab_client::apply::decide(
        flags.dry_run,
        flags.assume_yes || plan.deploy.is_empty(),
        answer.as_deref(),
    ) {
        homelab_client::apply::Decision::Preview => {
            println!("dry run — nothing deployed, nothing destroyed");
            std::process::exit(0);
        }
        homelab_client::apply::Decision::Decline => {
            die("not confirmed — nothing deployed, nothing destroyed")
        }
        homelab_client::apply::Decision::Deploy => {}
    }
}

/// arch-deploy-guard: every planned stack is checked before the
/// first one is sent, so a refusal leaves nothing half-applied.
async fn deploy_planned(
    host: &str,
    token: &str,
    flags: &ApplyFlags,
    plan: &homelab_client::apply::ApplyPlan,
    fleet: &FleetState,
    mut specs: BTreeMap<String, DeploySpec>,
) {
    for name in &plan.deploy {
        if let Err(e) = deploy_guard(&Path::new(&flags.base).join(name), name, fleet, flags.force) {
            die(&e);
        }
    }
    for name in &plan.deploy {
        let Some(sp) = specs.remove(name) else {
            continue;
        };
        if !deploy_spec(host, token, sp).await {
            die(&format!(
                "deploy of {} failed — apply stopped here; nothing after it was \
                 deployed and nothing was destroyed",
                name
            ));
        }
    }
}

/// Destroys each stack gone from the files, after its name was typed;
/// true when every destroy that ran succeeded.
async fn destroy_gone(host: &str, token: &str, skip_backup: bool, gone: &[String]) -> bool {
    let mut all_ok = true;
    if gone.is_empty() {
        return all_ok;
    }
    println!(
        "{}! {} stack(s) run on the host but are gone from the files. Each is \
         destroyed from the manifest the host recorded — backed up first{}, its \
         /appdata, backups and vault kept (see `homelab check`) — only after you \
         type its name. Enter keeps it.{}",
        C_YELLOW,
        gone.len(),
        if skip_backup {
            " (NOT: --no-backup)"
        } else {
            ""
        },
        C_RESET
    );
    for name in gone {
        let typed = read_typed(&format!("Type the stack name '{}' to destroy it: ", name));
        if &typed != name {
            println!("  kept {} — nothing destroyed", name);
            continue;
        }
        let destroy = Command::DestroyRecorded {
            stack: name.clone(),
            confirm: typed,
            skip_backup,
        };
        all_ok &= rpc_with(host, token, destroy).await;
    }
    all_ok
}

/// ask-8 (Kenny, 2026-09-27): the whole stacks directory against the
/// host. Changed stacks are deployed; a stack in host state whose
/// directory is gone is destroyed only after its name is typed. The
/// nightly round never does this — it never destroys anything.
pub(crate) async fn cmd_apply(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    let flags = ApplyFlags::parse(&c.args);
    let base_path = Path::new(&flags.base);
    if !base_path.is_dir() {
        die(&format!(
            "no stacks directory at '{}' — run this from the repository root, or pass \
             the path: homelab apply ~/Projects/homelab/stacks",
            flags.base
        ));
    }
    check_fleet_routes(base_path);
    let fleet = read_fleet(host, token).await;
    let side = host_side(&fleet);
    let local = build_local(base_path);
    let plan = homelab_client::apply::plan(&local.local, &local_dirs(base_path), &side.pairs);
    print_plan(host, token, &flags.base, &plan, &local, &side).await;
    // fix-142 (expert panel 2026-09-27, check-blind-to-repo-drift):
    // the plan alone, with an exit code a script can test.
    if flags.plan_only {
        std::process::exit(homelab_client::apply::plan_exit_code(&plan));
    }
    confirm_plan(&flags, &plan);
    deploy_planned(host, token, &flags, &plan, &fleet, local.specs).await;
    let all_ok = destroy_gone(host, token, flags.skip_backup, &plan.destroy).await;
    std::process::exit(if all_ok { 0 } else { 1 });
}
