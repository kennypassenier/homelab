//! Report verbs: status table, test plan, runbook.

use super::*;

/// Runs the verb `cmd` when it belongs to this module; false otherwise.
pub(crate) async fn dispatch(c: &Ctx, cmd: &str) -> bool {
    match cmd {
        "status" => cmd_status(c).await,
        "testplan" => cmd_testplan(c).await,
        "runbook" => cmd_runbook(c).await,
        _ => return false,
    }
    true
}

/// fix-68: a human table by default; `--json` still gets the
/// `FleetState` as-is, for a script (previously raw `pct list` +
/// the whole of state.json — 1,473 lines, measured live).
pub(crate) async fn cmd_status(c: &Ctx) {
    let host = &c.host;
    let token = &c.token;
    let args = &c.args;
    let json = args.iter().any(|a| a == "--json");
    let reply = rpc_reply(host, token, Command::Status)
        .await
        .unwrap_or_else(|| die("the host did not answer"));
    if !reply.ok {
        die(&reply.message);
    }
    if json {
        println!("{}", reply.message);
    } else {
        match serde_json::from_str::<homelab_proto::FleetState>(&reply.message) {
            Ok(fleet) => print!("{}", homelab_client::status::render_table(&fleet)),
            Err(_) => println!("{}", reply.message),
        }
    }
}

/// Phase 7's output document, derived from the tests rather than kept
/// beside them — the same reasoning as `runbook`.
pub(crate) async fn cmd_testplan(c: &Ctx) {
    let args = &c.args;
    let out = args.get(2).cloned().unwrap_or_else(|| {
        in_repo("docs/deployment/TEST_PLAN.md")
            .display()
            .to_string()
    });
    match homelab_client::testplan::generate_test_plan(
        &[&in_repo("core/tests"), &in_repo("client/tests")],
        &in_repo("docs/deployment/REALIZATION_PLAN.md"),
        &stacks_base(),
        Path::new(&out),
    ) {
        Ok(n) => println!(
            "{}✓ test plan written{} — {} ({} suite(s))",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&e),
    }
}

pub(crate) async fn cmd_runbook(c: &Ctx) {
    let args = &c.args;
    // E7: generate the disaster-recovery runbook from the local stacks
    // directory — a document that works when everything else is down.
    let out = args
        .get(2)
        .cloned()
        .unwrap_or_else(|| in_repo("docs/DR_RUNBOOK.md").display().to_string());
    match spec::generate_runbook(&stacks_base(), &out) {
        Ok(n) => println!(
            "{}✓ runbook written{} — {} ({} stack(s))",
            C_GREEN, C_RESET, out, n
        ),
        Err(e) => die(&format!("runbook: {}", e)),
    }
}
