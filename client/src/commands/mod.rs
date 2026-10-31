//! The `homelab` verbs: one function per verb, grouped by topic.

mod apply;
mod apply_plan;
mod backups;
mod check_print;
mod checks;
mod ctx;
mod deploy;
mod exchange;
mod exchange_frames;
mod exchange_print;
mod host_admin;
mod host_cmd;
mod manual_checks;
mod native_install;
mod new_stack;
mod plan;
mod reports;
mod restore;
mod rpc;
mod stacks;
mod startup;
mod teardown;
mod ui;
mod ui_guard;
mod ui_wait;
mod updates;

pub(crate) use apply_plan::*;
pub(crate) use check_print::*;
pub(crate) use ctx::*;
pub(crate) use deploy::*;
pub(crate) use exchange::*;
pub(crate) use exchange_print::*;
pub(crate) use host_cmd::*;
pub(crate) use native_install::*;
pub(crate) use new_stack::*;
pub(crate) use plan::*;
pub(crate) use restore::*;
pub(crate) use rpc::*;
pub(crate) use startup::*;
pub(crate) use teardown::*;
pub(crate) use ui_guard::*;
pub(crate) use ui_wait::*;

pub(crate) use homelab_client::output::Paint;
pub(crate) use std::path::Path;

pub(crate) use futures_util::{SinkExt, StreamExt};
pub(crate) use tokio_tungstenite::tungstenite::Message;

pub(crate) use homelab_client::{spec, tui};
pub(crate) use homelab_proto::{Command, LogLevel, RpcRequest, ServerMsg, UiStep};

/// Everything one verb needs: where the host is, how to prove who we are,
/// and the arguments as typed.
pub(crate) struct Ctx {
    pub(crate) host: String,
    pub(crate) token: String,
    pub(crate) args: Vec<String>,
    pub(crate) offline: bool,
    pub(crate) source: Option<String>,
}

/// Parses the invocation and runs the verb it names.
pub(crate) async fn run(explicit_host: Option<String>) {
    init_colour();
    let raw: Vec<String> = std::env::args().collect();
    // restore-two-sources-1: `--source local|remote` for the restore and
    // snapshot verbs, out of the way of every verb's own parser.
    let (args, source) = homelab_client::take_source(&raw).unwrap_or_else(|e| die(&e));
    // fix-68 (see REGISTER.md): bare `homelab` gives the morning answer
    // ("is anything waiting for me?"), the same one `homelab today` gives.
    let cmd = args.get(1).cloned().unwrap_or_else(|| "today".to_string());
    show_help_if_asked(&args);
    refuse_unknown_verb(&cmd);
    let host = resolve_host(explicit_host);
    let offline = args.iter().any(|a| a == "--offline" || a == "--demo");
    read_pre_answer(&args);
    let token = require_token(&cmd, offline);
    let ctx = Ctx {
        host,
        token,
        args,
        offline,
        source,
    };
    dispatch(&ctx, &cmd).await;
}

/// Hands the verb to the module that owns it.
async fn dispatch(c: &Ctx, cmd: &str) {
    let handled = host_admin::dispatch(c, cmd).await
        || stacks::dispatch(c, cmd).await
        || apply::dispatch(c, cmd).await
        || backups::dispatch(c, cmd).await
        || updates::dispatch(c, cmd).await
        || checks::dispatch(c, cmd).await
        || manual_checks::dispatch(c, cmd).await
        || reports::dispatch(c, cmd).await
        || ui::dispatch(c, cmd).await;
    if !handled {
        // fix-108: `help` (or no arguments). An unknown verb never reaches
        // here: it is refused before the token is read.
        print!("{}", homelab_client::cli_help::usage());
    }
}
