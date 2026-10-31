//! The plan and deploy verbs for one stack.

use super::*;

/// The per-file diff against what the host applied, printed.
pub(crate) async fn print_plan_diff(host: &str, token: &str, spec: &homelab_proto::DeploySpec) {
    let command = Command::GetApplied {
        stack: spec.manifest.stack_name.clone(),
    };
    match rpc_reply(host, token, command).await {
        Some(r) if r.ok => match serde_json::from_str::<Vec<homelab_proto::FileBlob>>(&r.message) {
            Ok(applied) => print_file_changes(spec, &applied),
            Err(_) => println!(
                "{}  (could not read what the host applied — the host answered: \
                 {}){}",
                C_DIM, r.message, C_RESET
            ),
        },
        Some(_not_ok) => println!(
            "{}  new: the host has never applied {} — the deploy would create CT \
             {}{}",
            C_DIM, spec.manifest.stack_name, spec.manifest.vmid, C_RESET
        ),
        None => println!(
            "{}  (the host did not answer — per-file diff skipped, validation above \
             stands){}",
            C_DIM, C_RESET
        ),
    }
}

pub(crate) fn print_file_changes(
    spec: &homelab_proto::DeploySpec,
    applied: &[homelab_proto::FileBlob],
) {
    let changes = homelab_client::apply::file_changes(&spec.files, applied);
    if changes.is_empty() {
        println!(
            "{}  files unchanged (secrets or settings differ){}",
            C_DIM, C_RESET
        );
    }
    for c in changes {
        println!("  {}", c);
    }
    for r in homelab_client::apply::native_restarts(&spec.files, applied) {
        println!("{}  ↻ {}{}", C_YELLOW, r, C_RESET);
    }
}

pub(crate) async fn cmd_plan(c: &Ctx) {
    let (host, token) = (&c.host, &c.token);
    // D6/D10: validate locally first — this much never needs the
    // network, and a bad stack file fails before anything is asked
    // of the host.
    let dir = &stack_dir(
        c.args
            .get(2)
            .unwrap_or_else(|| die("usage: homelab plan stacks/<name>")),
    );
    let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
    check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
    match homelab_core::manifest::validate(&spec) {
        Ok(()) => println!(
            "{}✓ valid{} — {} would deploy vmid {}: {} file(s), {} env(s)",
            C_GREEN,
            C_RESET,
            spec.manifest.stack_name,
            spec.manifest.vmid,
            spec.files.len(),
            spec.env.len()
        ),
        Err(e) => die(&format!("validation failed: {}", e)),
    }
    // fix-100 (apply-no-confirm-creates-drill, 2026-09-27): the
    // per-file diff this verb never had, now that `apply` has one
    // for every stack at once — this reads one stack's, with
    // HOMELAB_TOKEN set (D10's "no network" promise is kept when
    // it is not: validation above already ran and stands).
    if token.is_empty() {
        println!(
            "{}  (per-file diff needs HOMELAB_TOKEN — validation above stands on its \
             own){}",
            C_DIM, C_RESET
        );
    } else {
        print_plan_diff(host, token, &spec).await;
    }
}

pub(crate) async fn cmd_deploy(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let homelab_client::cli_args::Invocation::Deploy { stack, force } = invocation(args) else {
        die("internal: deploy parsed as another verb")
    };
    let dir = &stack_dir(&stack);
    let spec = spec::build_spec(Path::new(dir)).unwrap_or_else(|e| die(&e));
    // D10: fail fast client-side before opening a connection.
    if let Err(e) = homelab_core::manifest::validate(&spec) {
        die(&format!("validation failed: {}", e));
    }
    check_fleet_routes(Path::new(dir).parent().unwrap_or(Path::new(".")));
    // arch-deploy-guard: not over a deploy this tree has not seen.
    if !force {
        let (ok, fleet) = rpc_collect(host, token, Command::GetState).await;
        if let (true, Some(fleet)) = (ok, fleet)
            && let Err(e) = deploy_guard(Path::new(dir), &spec.manifest.stack_name, &fleet, false)
        {
            die(&e);
        }
    }
    let ok = deploy_spec(host, token, spec).await;
    std::process::exit(if ok { 0 } else { 1 });
}
