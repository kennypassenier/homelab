//! fix-63 (native-rebuild-starts-empty, expert panel, 2026-09-27): op-11 in
//! `docs/OPERATIONS_RUNBOOK.md` is a worked example a human types by hand at
//! 2am over an already-empty native service. Nothing proved the five lines
//! in that fenced block were still a coherent, safe sequence as the doc was
//! edited around them — a stray word, a swapped argument order, or the
//! `pct exec` vmid drifting from the `restic` path would only be found live.
//!
//! This test reads the block straight out of the doc (so a doc edit that
//! breaks the sequence fails here, not at 2am), runs each line through
//! [`MockExecutor`] the same way the real shell would (`sh -c` for every
//! pipeline), and checks the safety shape op-11's prose promises:
//! read-only commands before the "point of no return" comment, the unit
//! stopped before the archive is unpacked, and started again only after.

use homelab_core::executor::{Cmd, Executor, MockExecutor};

const RUNBOOK: &str = include_str!("../../docs/OPERATIONS_RUNBOOK.md");

/// Pulls the fenced ```sh block out of the op-11 "Worked example", by
/// finding the section heading and then the first fenced block after it.
/// Panics with a clear message if the doc's shape changed enough that
/// there is nothing left to extract — that is itself a finding, not a
/// silently-skipped test.
fn op11_worked_example() -> Vec<(String, bool)> {
    let heading = "## op-11 ";
    let section_start = RUNBOOK
        .find(heading)
        .expect("docs/OPERATIONS_RUNBOOK.md no longer has an op-11 section");
    let after = &RUNBOOK[section_start..];
    let fence_start = after
        .find("```sh")
        .expect("op-11 no longer has a ```sh worked example");
    let block_start = fence_start + "```sh".len();
    let block_end = after[block_start..]
        .find("```")
        .expect("op-11's ```sh block is never closed");
    let block = &after[block_start..block_start + block_end];

    block
        .lines()
        .map(str::trim)
        .filter(|l| !l.is_empty())
        .map(|l| {
            // Strip a trailing `# comment` (not inside quotes — none of
            // op-11's lines quote a literal `#`).
            let code = l.split('#').next().unwrap_or(l).trim().to_string();
            let is_point_of_no_return = l.contains("point of no return");
            (code, is_point_of_no_return)
        })
        .filter(|(code, _)| !code.is_empty())
        .collect()
}

#[tokio::test]
async fn fix_63_op11_s_worked_example_runs_clean_against_the_mock() {
    let lines = op11_worked_example();
    assert!(
        lines.len() >= 5,
        "expected at least the export/snapshots/dump/stop/dump|tar/start lines, got {:?}",
        lines
    );

    let exec = MockExecutor::new();
    let mut executed = Vec::new();
    for (line, _) in &lines {
        if line.starts_with("export ") {
            // A shell builtin, not a command op-11 runs against the host or
            // the container — nothing for the executor to record.
            continue;
        }
        let out = exec
            .run(&Cmd::new("sh", &["-c", line], 60))
            .await
            .unwrap_or_else(|e| panic!("op-11 line failed to run at all: {line:?}: {e}"));
        assert!(
            out.success(),
            "op-11 line did not succeed against the mock: {line:?}"
        );
        executed.push(line.clone());
    }

    // Every real line ran (only the `export`s were skipped).
    assert_eq!(
        executed.len(),
        lines
            .iter()
            .filter(|(l, _)| !l.starts_with("export "))
            .count()
    );

    // Safety shape: a read-only look at the archive before anything is
    // stopped or touched.
    let first_destructive = executed
        .iter()
        .position(|l| l.contains("systemctl stop"))
        .expect("op-11 must stop the unit before unpacking into it");
    let read_only_look = executed
        .iter()
        .position(|l| l.contains("tar -tvf"))
        .expect("op-11 must show a read-only listing of the archive first");
    assert!(
        read_only_look < first_destructive,
        "the read-only look at the archive must come before the unit is stopped"
    );

    // The unit is stopped before the archive is unpacked into it, and
    // started again only after.
    let unpack = executed
        .iter()
        .position(|l| l.contains("tar -xf"))
        .expect("op-11 must unpack the archive into the container");
    let restart = executed
        .iter()
        .position(|l| l.contains("systemctl start"))
        .expect("op-11 must start the unit again once the data is back");
    assert!(
        first_destructive < unpack,
        "the unit must be stopped before its data directory is overwritten"
    );
    assert!(
        unpack < restart,
        "the unit must start again only after the archive is unpacked"
    );

    // The same container the archive is unpacked into is the one that was
    // stopped and the one that is started again — a copy-pasted vmid from
    // another example would restore one container's data into a different
    // running one.
    let stop_vmid = executed[first_destructive]
        .split_whitespace()
        .nth(2)
        .expect("`pct exec <vmid> -- systemctl stop ...`");
    let start_vmid = executed[restart]
        .split_whitespace()
        .nth(2)
        .expect("`pct exec <vmid> -- systemctl start ...`");
    assert_eq!(
        stop_vmid, start_vmid,
        "op-11 must stop and start the same container ({stop_vmid} vs {start_vmid})"
    );
    assert!(
        executed[unpack].contains(&format!("pct exec {}", stop_vmid)),
        "op-11 must unpack into the same container it stopped ({stop_vmid}): {:?}",
        executed[unpack]
    );
}

#[test]
fn fix_63_op11_marks_exactly_one_point_of_no_return() {
    let lines = op11_worked_example();
    let marked = lines.iter().filter(|(_, poof)| *poof).count();
    assert_eq!(
        marked, 1,
        "op-11 should mark exactly the first command with no going back \
         (the service stop) as the point of no return, found {marked}"
    );
}
