//! redesign-flows-6 (the senior review of the 3.71.0 Update flow, items 3
//! and 4): the Update flow is ONE job on the dashboard's queue — back up,
//! commit, deploy, verify — and its verify is real: a touched stack with a
//! container down, or a pinned app not at its new version, is not healthy,
//! and the safety net then puts the earlier image line back by itself. The
//! job keeps a record the stack's History offers to undo for 7 days.

mod act_support;

use std::collections::BTreeMap;
use std::sync::{Arc, Mutex};
use std::time::Duration;

use act_support::{MemFiles, MockHost, RecPusher, Recorder, Script, TestClock, shared, until};
use homelab_admin::core::actions::{ActionArgs, Refusal, validate};
use homelab_admin::core::updateflow::{self, ItemKind, UpdateItem, judge};
use homelab_admin::shell::actions::{
    Actions, ActionsDeps, ImageCommitter, JobState, JobView, Origin,
};
use homelab_admin::shell::actions_notify::NotifyCenter;
use homelab_core::ops::pins::{RunningImage, StackRuntime};
use homelab_proto::Command;

const FROM: &str = "example/demo-api:v2.3.0@sha256:aaaa";
const TO: &str = "example/demo-api:v3.0.0@sha256:bbbb";

fn pin() -> UpdateItem {
    UpdateItem {
        stack: "media".into(),
        kind: ItemKind::Pin,
        key: Some("api/api".into()),
        app: Some("demo-api".into()),
        from: Some(FROM.into()),
        to: Some(TO.into()),
    }
}

fn runtime(up: bool, image: &str) -> StackRuntime {
    StackRuntime {
        containers: vec![("demo-api".into(), up), ("demo-web".into(), true)],
        images: BTreeMap::from([(
            "demo-api".to_string(),
            RunningImage {
                image: format!("registry-cache.lan/{image}"),
                digest: image.split_once('@').map(|x| x.1).unwrap_or("").into(),
                upstream: None,
                seen_at: 0,
            },
        )]),
    }
}

/// The verdict: running counts and the version, never "always green".
#[test]
fn redesign_flows_6_the_verdict_reads_containers_and_the_running_version() {
    let items = vec![pin()];
    let rt = |r: Result<StackRuntime, String>| BTreeMap::from([("media".to_string(), r)]);
    let ok = judge(&items, &rt(Ok(runtime(true, TO))));
    assert!(ok.healthy, "{:?}", ok.words);
    assert!(
        ok.words
            .iter()
            .any(|w| w.contains("reports version v3.0.0"))
    );
    // A container down: not healthy, and it says which.
    let down = judge(&items, &rt(Ok(runtime(false, TO))));
    assert!(!down.healthy);
    assert!(
        down.words
            .iter()
            .any(|w| w.contains("1 of 2") && w.contains("demo-api"))
    );
    // Still the old image: not healthy.
    let old = judge(&items, &rt(Ok(runtime(true, FROM))));
    assert!(!old.healthy);
    assert!(
        old.words.iter().any(|w| w.contains("does not run v3.0.0")),
        "{:?}",
        old.words
    );
    // Unreadable: not healthy; a host too old to know the read: said, not
    // invented.
    assert!(!judge(&items, &rt(Err("ssh: timeout".into()))).healthy);
    let older = judge(
        &items,
        &rt(Err(
            "unknown variant `stack_runtime`, expected one of …".into()
        )),
    );
    assert!(older.healthy);
    assert!(older.words[0].contains("does not report the running version"));
}

/// The moves are checked before anything runs.
#[test]
fn redesign_flows_6_the_moves_are_checked_before_the_job_runs() {
    let ok = serde_json::to_string(&vec![pin()]).unwrap();
    assert!(
        validate(
            "_host",
            "update-apps",
            ActionArgs {
                updates: Some(ok),
                ..Default::default()
            }
        )
        .is_ok()
    );
    for bad in [
        "",
        "[]",
        r#"[{"stack":"Media","kind":"pull"}]"#,
        r#"[{"stack":"media","kind":"pin"}]"#,
    ] {
        assert!(
            validate(
                "_host",
                "update-apps",
                ActionArgs {
                    updates: Some(bad.into()),
                    ..Default::default()
                }
            )
            .is_err(),
            "{bad}"
        );
    }
    // Never a catalog action: only the Update flow starts it.
    assert!(
        !homelab_admin::core::actions::catalog()
            .iter()
            .any(|e| e.action.slug() == "update-apps")
    );
}

struct Commits(Mutex<Vec<(String, BTreeMap<String, String>)>>);

impl ImageCommitter for Commits {
    fn commit_images(
        &self,
        stack: String,
        images: BTreeMap<String, String>,
        _subject: String,
        _origin: Origin,
    ) -> std::pin::Pin<Box<dyn std::future::Future<Output = Result<String, Refusal>> + Send + '_>>
    {
        Box::pin(async move {
            let mut c = self.0.lock().unwrap();
            c.push((stack, images));
            Ok(format!("{:0>40}", c.len()))
        })
    }
}

struct World {
    host: Arc<MockHost>,
    actions: Actions,
    commits: Arc<Commits>,
}

/// `running`: what the stack's containers run after N deploys of a commit.
fn world(
    _tag: &str,
    backup_ok: bool,
    running: Arc<dyn Fn(usize) -> StackRuntime + Send + Sync>,
) -> World {
    let clock = TestClock::at(1_790_000_000);
    let deploys = Arc::new(Mutex::new(0usize));
    let d2 = deploys.clone();
    let host = MockHost::start(
        clock.clone(),
        Arc::new(move |c: &Command| match c {
            Command::BackupStack(_) if !backup_ok => {
                Script::failed(&[("snapshot", 5)], "restic: repository locked")
            }
            Command::DeployStack(_) => {
                *d2.lock().unwrap() += 1;
                Script::ok(&[("pull", 5), ("up", 5), ("verify", 2)])
            }
            Command::StackRuntime { .. } => Script {
                message: serde_json::json!({ "runtime": running(*d2.lock().unwrap()) }).to_string(),
                ..Script::ok(&[])
            },
            _ => Script::ok(&[("snapshot", 5)]),
        }),
        serde_json::json!({ "entries": [] }),
        Arc::new(Mutex::new(Vec::new())),
    );
    let live = Arc::new(Recorder::default());
    let db = homelab_admin::shell::store::memory_db();
    let notify = NotifyCenter::load(
        db.clone(),
        Arc::new(RecPusher::default()),
        live.clone(),
        clock.clock(),
    )
    .unwrap();
    let actions = Actions::start(ActionsDeps {
        host: host.clone(),
        publish: live,
        files: Arc::new(MemFiles::default()),
        shared: shared(&[("media", 106, None)]),
        notify,
        clock: clock.clock(),
        timeout: Duration::from_secs(5),
    });
    let commits = Arc::new(Commits(Mutex::new(Vec::new())));
    actions.set_committer(commits.clone());
    actions.set_flow_store(db);
    actions.set_flow_timing(Duration::from_millis(20), Duration::from_millis(200));
    World {
        host,
        actions,
        commits,
    }
}

async fn run(w: &World) -> JobView {
    let req = validate(
        "_host",
        "update-apps",
        ActionArgs {
            updates: Some(serde_json::to_string(&vec![pin()]).unwrap()),
            ..Default::default()
        },
    )
    .unwrap();
    let job = w.actions.press(req, Origin::Manual).await.unwrap().job;
    until("the update job to finish", || {
        w.actions.job(job).is_some_and(|j| j.state.finished())
    })
    .await;
    w.actions.job(job).unwrap()
}

fn names(w: &World) -> Vec<String> {
    w.host.ran().into_iter().map(|(_, n, _)| n).collect()
}

fn rows(j: &JobView) -> Vec<(String, String)> {
    j.flow.as_ref().unwrap()["rows"]
        .as_array()
        .unwrap()
        .iter()
        .map(|r| {
            (
                r["id"].as_str().unwrap().to_string(),
                r["state"].as_str().unwrap().to_string(),
            )
        })
        .collect()
}

/// Healthy at the new version: one job did back up → commit → deploy →
/// verify, it is a job like every other, and the stack's History offers the
/// undo.
#[tokio::test]
async fn redesign_flows_6_one_job_backs_up_commits_deploys_and_verifies() {
    let w = world("flow-ok", true, Arc::new(|_| runtime(true, TO)));
    let j = run(&w).await;
    assert_eq!(j.state, JobState::Done, "{:?}", j.message);
    assert_eq!(j.action.slug(), "update-apps");
    let ran = names(&w);
    assert_eq!(
        ran.first().map(String::as_str),
        Some("backup_stack"),
        "{ran:?}"
    );
    let deploy = ran
        .iter()
        .position(|n| n == "deploy_stack")
        .expect("a deploy");
    let verify = ran
        .iter()
        .position(|n| n == "stack_runtime")
        .expect("a verify read");
    assert!(deploy < verify, "{ran:?}");
    assert_eq!(w.commits.0.lock().unwrap()[0].1["api/api"], TO);
    assert!(rows(&j).iter().all(|(_, s)| s == "ok"), "{:?}", rows(&j));
    // flow-stage-by-stage-1: one row per stack in each stage.
    assert_eq!(
        rows(&j)
            .iter()
            .map(|(id, _)| id.as_str())
            .collect::<Vec<_>>(),
        ["backup:media", "commit:media", "deploy:media", "health"]
    );
    let undo = w.actions.update_records(Some("media")).await;
    assert_eq!(undo.len(), 1);
    assert_eq!(undo[0].job, j.job);
    assert!(w.actions.update_records(Some("other")).await.is_empty());
}

/// A failed backup: nothing else happens.
#[tokio::test]
async fn redesign_flows_6_a_failed_backup_changes_nothing() {
    let w = world("flow-backup", false, Arc::new(|_| runtime(true, TO)));
    let j = run(&w).await;
    assert_eq!(j.state, JobState::Failed);
    assert_eq!(names(&w), vec!["backup_stack"]);
    assert!(w.commits.0.lock().unwrap().is_empty());
    assert!(j.message.unwrap().contains("Nothing was changed"));
}

/// Not healthy within the window: the health row is bad, the safety net
/// commits the earlier line and deploys it, and the job says so.
#[tokio::test]
async fn redesign_flows_6_not_healthy_rolls_the_earlier_version_back() {
    // After the update's deploy a container is down; after the roll back's
    // deploy all run the old version again.
    let w = world(
        "flow-rollback",
        true,
        Arc::new(|deploys| {
            if deploys >= 2 {
                runtime(true, FROM)
            } else {
                runtime(false, TO)
            }
        }),
    );
    let j = run(&w).await;
    assert_eq!(j.state, JobState::Failed);
    let r = rows(&j);
    assert!(r.contains(&("health".into(), "bad".into())), "{r:?}");
    assert!(r.contains(&("rollback".into(), "ok".into())), "{r:?}");
    let commits = w.commits.0.lock().unwrap().clone();
    assert_eq!(commits.len(), 2);
    assert_eq!(
        commits[1].1["api/api"], FROM,
        "the roll back commits the earlier line"
    );
    assert_eq!(names(&w).iter().filter(|n| *n == "deploy_stack").count(), 2);
    assert!(j.flow.as_ref().unwrap()["rolled_back"].as_bool().unwrap());
    assert!(j.message.unwrap().contains("put the earlier version back"));
    // A rolled-back update is not offered to undo.
    assert!(w.actions.update_records(Some("media")).await.is_empty());
}

/// Still the old version after the deploy (the image did not change): not
/// healthy either.
#[tokio::test]
async fn redesign_flows_6_the_old_version_still_running_is_not_healthy() {
    let w = world("flow-old", true, Arc::new(|_| runtime(true, FROM)));
    let j = run(&w).await;
    assert_eq!(j.state, JobState::Failed);
    assert!(rows(&j).contains(&("health".into(), "bad".into())));
}

/// The undo list offers 7 days; older rows stay stored (only the Storage
/// page deletes them).
#[test]
fn redesign_flows_6_the_undo_offers_seven_days() {
    let rec = |at: i64, outcome: &str| updateflow::UpdateRecord {
        job: at as u64,
        at,
        by: "x".into(),
        items: vec![pin()],
        commits: vec![],
        outcome: outcome.into(),
    };
    let now = 100 * 86_400;
    let all = vec![
        rec(now - 8 * 86_400, "done"),
        rec(now - 86_400, "done"),
        rec(now, "failed"),
    ];
    let u = updateflow::undoable(&all, Some("media"), now);
    assert_eq!(u.len(), 1);
    assert_eq!(u[0].at, now - 86_400);
}

/// flow-stage-by-stage-1 (Kenny, 2026-10-24: "maak dat dan voor vijf apps
/// stap 1-5, app per app. Ga dan verder naar de volgende stage"): every
/// stack's backup, then every commit, then every deploy, then the checks;
/// a pull-only stack has no commit row.
#[test]
fn flow_stage_by_stage_1_rows_go_stage_by_stage_and_stack_by_stack() {
    let pin = |stack: &str| UpdateItem {
        stack: stack.into(),
        kind: ItemKind::Pin,
        key: Some("api/api".into()),
        app: Some("api".into()),
        from: Some("a".into()),
        to: Some("b".into()),
    };
    let pull = UpdateItem {
        stack: "media".into(),
        kind: ItemKind::Pull,
        key: None,
        app: Some("jellyfin".into()),
        from: None,
        to: None,
    };
    let f = updateflow::FlowView::new(vec![pin("gateway"), pull, pin("downloader")]);
    let ids: Vec<&str> = f.rows.iter().map(|r| r.id.as_str()).collect();
    let stacks = updateflow::stacks_of(&f.items);
    let mut want: Vec<String> = stacks.iter().map(|s| format!("backup:{s}")).collect();
    want.extend(
        stacks
            .iter()
            .filter(|s| s.as_str() != "media")
            .map(|s| format!("commit:{s}")),
    );
    want.extend(stacks.iter().map(|s| format!("deploy:{s}")));
    want.push("health".into());
    assert_eq!(ids, want.iter().map(String::as_str).collect::<Vec<_>>());
}

/// progress-order-1 (Kenny, 2026-10-24, a screenshot with Back up done, the
/// logs check an empty circle and "Change the files and commit" running):
/// the list is in the order the job runs, and as the job moves along exactly
/// one row runs, every row before it is done and every row after it waits.
#[test]
fn progress_order_1_one_row_runs_at_a_time_in_run_order() {
    use homelab_admin::core::updateflow::RowState;
    let pin = |stack: &str| UpdateItem {
        stack: stack.into(),
        kind: ItemKind::Pin,
        key: Some("api/api".into()),
        app: Some("api".into()),
        from: Some("a".into()),
        to: Some("b".into()),
    };
    let mut f = updateflow::FlowView::new(vec![pin("gateway"), pin("downloader")]);
    let order: Vec<String> = f.rows.iter().map(|r| r.id.clone()).collect();
    // The run order of the shell (shell/actions_flow.rs): every backup, every
    // commit, every deploy, then the health check.
    let ran: Vec<String> = ["backup", "commit", "deploy"]
        .iter()
        .flat_map(|stage| {
            updateflow::stacks_of(&f.items)
                .into_iter()
                .map(move |s| format!("{stage}:{s}"))
        })
        .chain(std::iter::once("health".to_string()))
        .collect();
    assert_eq!(order, ran, "the list is the run order");
    for (i, id) in ran.iter().enumerate() {
        f.set(id, RowState::Run, None, 100 + i as i64);
        let states: Vec<RowState> = f.rows.iter().map(|r| r.state).collect();
        assert_eq!(
            states.iter().filter(|s| **s == RowState::Run).count(),
            1,
            "{id} runs alone: {states:?}"
        );
        assert!(states[..i].iter().all(|s| *s == RowState::Ok), "{states:?}");
        assert!(
            states[i + 1..].iter().all(|s| *s == RowState::Wait),
            "{states:?}"
        );
        f.set(id, RowState::Ok, None, 101 + i as i64);
    }
}
