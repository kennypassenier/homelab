//! Frames this end cannot read, unknown fields, self-update acceptance, masked exec and private files.

use super::support::*;
use crate::*;

/// fix-141 (expert panel 2026-09-27, changes-reach-prod-without-ci): the
/// Hello every client prints names the build, not only the version.
#[test]
pub(crate) fn fix_141_the_hello_names_the_build() {
    match crate::hello() {
        homelab_proto::ServerMsg::Hello { version, build, .. } => {
            assert_eq!(version, env!("CARGO_PKG_VERSION"));
            assert_eq!(build.as_deref(), option_env!("HOMELAB_BUILD"));
            assert!(build.is_some_and(|b| !b.is_empty()));
        }
        other => panic!("not a Hello: {other:?}"),
    }
}

/// fix-55 (expert panel, unparseable-frame-logged-whole, 2026-09-27): a
/// frame the host could not parse was logged whole. A deploy frame carries
/// every `.env` value of its stack (`DeploySpec.env` is the secrets
/// channel) and a staging frame tens of MB of base64, so one version skew
/// put a stack's secrets, or a whole binary, into one pve journal line.
#[test]
pub(crate) fn fix_55_an_unparseable_frame_is_logged_without_its_body() {
    let secret = "hunter2-db-password";
    let frame = format!(
        r#"{{"id":7,"cmd":"deploy_stack","manifest":{{"vmid":"not-a-number"}},"env":{{"app":{{"DB_PASSWORD":"{}"}}}},"blob":"{}"}}"#,
        secret,
        "A".repeat(100_000)
    );
    let e = serde_json::from_str::<RpcRequest>(&frame).unwrap_err();
    let line = crate::unparseable_frame_line(&e, &frame);
    assert!(!line.contains(secret), "{}", line);
    assert!(line.len() < 500, "{} bytes", line.len());
    assert!(line.contains("deploy_stack"), "names the method: {}", line);
    assert!(
        line.contains(&frame.len().to_string()),
        "and the size: {}",
        line
    );
    // serde's own message quotes the offending value; that value may be a
    // secret too, so only its position is kept.
    let e =
        serde_json::from_str::<RpcRequest>(r#"{"id":"s3cr3t-token","cmd":"ping"}"#).unwrap_err();
    let line = crate::unparseable_frame_line(&e, r#"{"id":"s3cr3t-token","cmd":"ping"}"#);
    assert!(!line.contains("s3cr3t-token"), "{}", line);
    // The positive twin: still a real diagnostic line, not an empty one
    // that would also pass "no secret in it".
    assert!(!line.is_empty(), "the frame still gets a log line");
}

/// covers: fix-211
///
/// (1) a mutating command carrying a field this host's own build has
/// no struct field for ("force_wipe" is not a field of
/// `set_stack_enabled`) is refused, naming that field's exact path,
/// before it ever reaches a handler.
#[test]
pub(crate) fn fix_211_a_field_the_host_does_not_know_is_refused_by_name() {
    let (value, req) = parsed(
        r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true,"force_wipe":true}"#,
    );
    let why = crate::unknown_field_refusal(&value, &req).expect("refused");
    assert!(why.contains("force_wipe"), "{}", why);
    assert!(why.contains("release-update"), "{}", why);
}

/// covers: fix-211
///
/// (2) a mutating command whose JSON carries only fields this build
/// knows is never refused by this check.
#[test]
pub(crate) fn fix_211_a_command_with_only_known_fields_is_never_refused() {
    let (value, req) =
        parsed(r#"{"id":1,"cmd":"set_stack_enabled","stack":"media","enabled":true}"#);
    assert!(crate::unknown_field_refusal(&value, &req).is_none());
}

/// covers: fix-211
///
/// A READ-ONLY command carrying a field this build does not know is
/// never refused: it changes nothing, so an unknown field in it
/// cannot repeat the 2026-08-31 incident.
#[test]
pub(crate) fn fix_211_a_read_only_command_is_never_refused_for_an_unknown_field() {
    let (value, req) = parsed(r#"{"id":1,"cmd":"status","mystery_field":true}"#);
    assert!(req.command.is_read_only());
    assert!(crate::unknown_field_refusal(&value, &req).is_none());
}

/// fix-121 (expert panel, self-update-acceptance-weak, 2026-09-27): a new
/// daemon was accepted after five seconds alive, before anything had
/// talked to it, so a binary that ran but could not serve a client was
/// kept. It is accepted now by the first authenticated request it
/// answers.
#[tokio::test]
pub(crate) async fn fix_121_a_self_update_is_accepted_by_an_answered_request() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-new", now - 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    assert!(marker.exists(), "precondition: the update is pending");
    ping_over(addr).await;
    let accepted = !marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(
        accepted,
        "an answered request from the new daemon accepts the update"
    );
}

/// fix-121: the daemon that ARMED the marker answers the self-update
/// request itself; that answer must not accept the binary that replaces
/// it.
#[tokio::test]
pub(crate) async fn fix_121_the_old_daemon_never_accepts_its_successor() {
    let now = std::time::SystemTime::now()
        .duration_since(std::time::UNIX_EPOCH)
        .unwrap()
        .as_secs();
    let (state, marker) = state_with_marker("fix121-old", now + 100);
    let addr =
        serve_state_on_loopback(state, |st, req| async move { handle_rpc(&st, req).await }).await;
    ping_over(addr).await;
    let kept = marker.exists();
    let _ = std::fs::remove_dir_all(marker.parent().unwrap());
    assert!(kept, "armed after this daemon started: not its own");
}

/// fix-124 (expert panel, exec-logged-verbatim, 2026-09-27): a remote
/// exec is recorded in audit.log and the journal before it runs, and was
/// recorded verbatim, so a password typed on the command line was kept
/// on pve indefinitely. The record still names the vmid and the command,
/// with the values the shared masker recognises taken out.
#[test]
pub(crate) fn fix_124_an_exec_command_is_masked_before_it_is_recorded() {
    let line = exec_audit_line(
        1_800_000_000,
        108,
        "PGPASSWORD=hunter2-x9 psql -h db -c 'select 1'; curl -H 'Authorization: Bearer tk-77' x",
    );
    assert!(
        !line.contains("hunter2-x9") && !line.contains("tk-77"),
        "{}",
        line
    );
    assert!(
        line.starts_with("1800000000 exec vmid=108 ") && line.contains("psql -h db"),
        "the record still says what ran where: {}",
        line
    );
    assert!(
        line.ends_with('\n') && line.lines().count() == 1,
        "{:?}",
        line
    );
}

/// fix-125 (expert panel, bundles-audit-world-readable, 2026-09-27):
/// audit.log (every exec command) and the incident bundles were readable
/// by any local account on pve. A new audit.log is created 0600, and at
/// start the daemon takes group and world access off what exists.
#[test]
pub(crate) fn fix_125_the_audit_log_and_existing_bundles_are_made_private() {
    use std::os::unix::fs::PermissionsExt;
    let mode = |p: &std::path::Path| std::fs::metadata(p).unwrap().permissions().mode() & 0o777;
    let set = |p: &std::path::Path, m: u32| {
        std::fs::set_permissions(p, std::fs::Permissions::from_mode(m)).unwrap()
    };
    let dir = std::env::temp_dir().join(format!("homelab-fix125-{}", std::process::id()));
    let _ = std::fs::remove_dir_all(&dir);
    let bundle = dir.join("incidents/1800000000-deploy-x");
    std::fs::create_dir_all(&bundle).unwrap();
    for (f, m) in [("report.json", 0o644), ("commands.sh", 0o755)] {
        std::fs::write(bundle.join(f), "x").unwrap();
        set(&bundle.join(f), m);
    }
    set(&dir.join("incidents"), 0o755);
    set(&bundle, 0o755);
    std::fs::write(dir.join("audit.log"), "old\n").unwrap();
    set(&dir.join("audit.log"), 0o644);

    tighten_private_paths(&dir.to_string_lossy());
    let fresh = dir.join("fresh-audit.log");
    append_audit(&fresh.to_string_lossy(), "1 exec vmid=108 cmd=\"ls\"\n").unwrap();

    let got = [
        mode(&dir.join("incidents")),
        mode(&bundle),
        mode(&bundle.join("report.json")),
        mode(&bundle.join("commands.sh")),
        mode(&dir.join("audit.log")),
        mode(&fresh),
    ];
    let _ = std::fs::remove_dir_all(&dir);
    assert_eq!(got, [0o700, 0o700, 0o600, 0o700, 0o600, 0o600]);
}
