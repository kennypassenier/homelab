//! key-move-1: a commit in the host's clone that the remote lacks (the host
//! stopped between commit and push) keeps every request refused, names the
//! two commands that settle it, and leaves ONE quiet Inbox notice that
//! repeated refusals do not stack. Throwaway bare repositories only.

use super::stack_commit::{TOKEN, World, git};
use super::support::*;
use crate::*;
use homelab_core::ops::stackcommit::StackFileChange;
use std::path::Path;

fn commit_rpc(base: &str) -> RpcRequest {
    RpcRequest {
        id: 11,
        command: Rpc::CommitStackFiles {
            message: "m [key-move]".into(),
            base: base.into(),
            files: vec![StackFileChange {
                path: "stacks/media/lxc-compose.yml".into(),
                content: Some("again\n".into()),
            }],
            push: true,
        },
    }
}

fn open_notices(state_dir: &Path) -> Vec<homelab_core::notify::HostNotice> {
    let db = crate::hostdb::for_dir(&state_dir.display().to_string()).unwrap();
    crate::hostdb::notices::all(&db)
        .unwrap()
        .into_iter()
        .filter(|n| n.label == crate::stack_commit::notice::LABEL)
        .collect()
}

#[tokio::test]
pub(crate) async fn key_move_1_an_unpushed_commit_is_refused_and_noticed_once() {
    let w = World::new("unpushed", None);
    // The clone exists after one request; then the host "crashes" with a
    // local commit that never reached the remote.
    crate::stack_commit::commit(
        &w.cfg,
        &w.request(vec![("stacks/media/lxc-compose.yml", Some("a\n"))], true),
    )
    .unwrap();
    let clone = Path::new(&w.cfg.dir);
    git(
        clone,
        &["commit", "--allow-empty", "--quiet", "-m", "lost edit"],
    );
    let local = git(clone, &["rev-parse", "HEAD"]);
    let remote = w.remote_head();

    let state_dir = w.root.join("state");
    std::fs::create_dir_all(&state_dir).unwrap();
    let raw = format!(
        "{TOKEN}state_dir = {:?}\n[stack_commits]\nremote = {:?}\nkey = \"/nonexistent\"\n",
        state_dir.display().to_string(),
        w.bare.display().to_string()
    );
    let state = test_state(config_from_text(&raw));
    for _ in 0..3 {
        let r = crate::rpc::handle_rpc(&state, commit_rpc(&remote)).await;
        assert!(!r.ok, "{}", r.message);
        for want in [
            &local[..12],
            "lost edit",
            "git push origin HEAD:refs/heads/main",
            "git reset --hard origin/main",
            "as root on the host",
        ] {
            assert!(r.message.contains(want), "{want}: {}", r.message);
        }
    }
    let notices = open_notices(&state_dir);
    assert_eq!(notices.len(), 1, "{notices:?}");
    let n = &notices[0];
    assert!(!n.urgent && n.what.contains(&local[..12]) && n.what.contains("lost edit"));
    assert!(
        n.remedy.contains("git push origin") && n.remedy.contains("git reset --hard origin/main")
    );
    // Never fixed by itself.
    assert_eq!(git(clone, &["rev-parse", "HEAD"]), local);
    assert_eq!(w.remote_head(), remote);

    // A different unpushed commit replaces the notice instead of adding one.
    git(
        clone,
        &["commit", "--allow-empty", "--quiet", "-m", "second lost"],
    );
    crate::rpc::handle_rpc(&state, commit_rpc(&remote)).await;
    let notices = open_notices(&state_dir);
    assert_eq!(notices.len(), 1, "{notices:?}");
    assert!(notices[0].what.contains("second lost"));
}
