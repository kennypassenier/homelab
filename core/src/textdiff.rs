//! feat-stacks-2: the plan's file diff. A line diff (longest common
//! subsequence) cut into hunks with a few lines of context, as `git diff`
//! shows it. Pure.
//!
//! fix-219 (drift-finding-names-only-filenames, 2026-10-02): moved here from
//! `homelab-admin` so the CLI (`homelab check`/`today`) and the host can show
//! the same diff the dashboard's Apply page already built for a commit plan
//! — one diff engine, never two that could disagree about what "changed"
//! means. `admin::core::textdiff` is now a thin re-export of this module.

use serde::Serialize;

/// One line of a hunk: `=` unchanged, `-` only in the old text, `+` only in
/// the new one.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct DiffLine {
    pub op: char,
    /// Line number in the old text (1-based), for `=` and `-`.
    pub old: Option<usize>,
    /// Line number in the new text (1-based), for `=` and `+`.
    pub new: Option<usize>,
    pub text: String,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Hunk {
    pub old_start: usize,
    pub old_len: usize,
    pub new_start: usize,
    pub new_len: usize,
    pub lines: Vec<DiffLine>,
}

/// Beyond this many line pairs the table would be too large to hold; the
/// diff then shows the whole file replaced, which is still true.
const MAX_CELLS: usize = 4_000_000;

/// Every line of both texts, marked.
pub fn line_ops(old: &str, new: &str) -> Vec<DiffLine> {
    let a: Vec<&str> = old.lines().collect();
    let b: Vec<&str> = new.lines().collect();
    // The common head and tail need no table.
    let head = a.iter().zip(&b).take_while(|(x, y)| x == y).count();
    let tail = a[head..]
        .iter()
        .rev()
        .zip(b[head..].iter().rev())
        .take_while(|(x, y)| x == y)
        .count();
    let (am, bm) = (&a[head..a.len() - tail], &b[head..b.len() - tail]);
    let mut mid: Vec<(char, Option<usize>, Option<usize>)> = Vec::new();
    if am.len().saturating_mul(bm.len()) > MAX_CELLS {
        mid.extend((0..am.len()).map(|i| ('-', Some(i), None)));
        mid.extend((0..bm.len()).map(|j| ('+', None, Some(j))));
    } else {
        // lcs[i][j]: the longest common run of am[i..] and bm[j..].
        let (n, m) = (am.len(), bm.len());
        let mut lcs = vec![0u32; (n + 1) * (m + 1)];
        let at = |i: usize, j: usize| i * (m + 1) + j;
        for i in (0..n).rev() {
            for j in (0..m).rev() {
                lcs[at(i, j)] = if am[i] == bm[j] {
                    lcs[at(i + 1, j + 1)] + 1
                } else {
                    lcs[at(i + 1, j)].max(lcs[at(i, j + 1)])
                };
            }
        }
        let (mut i, mut j) = (0, 0);
        while i < n || j < m {
            if i < n && j < m && am[i] == bm[j] {
                mid.push(('=', Some(i), Some(j)));
                i += 1;
                j += 1;
            } else if i < n && (j == m || lcs[at(i + 1, j)] >= lcs[at(i, j + 1)]) {
                // The old line first, as `git diff` shows a change.
                mid.push(('-', Some(i), None));
                i += 1;
            } else {
                mid.push(('+', None, Some(j)));
                j += 1;
            }
        }
    }
    let mut out = Vec::with_capacity(a.len().max(b.len()));
    for (k, text) in a.iter().enumerate().take(head) {
        out.push(DiffLine {
            op: '=',
            old: Some(k + 1),
            new: Some(k + 1),
            text: text.to_string(),
        });
    }
    for (op, i, j) in mid {
        let text = match (i, j) {
            (Some(i), _) => am[i],
            (None, Some(j)) => bm[j],
            _ => "",
        };
        out.push(DiffLine {
            op,
            old: i.map(|i| head + i + 1),
            new: j.map(|j| head + j + 1),
            text: text.to_string(),
        });
    }
    for k in 0..tail {
        let (oi, ni) = (a.len() - tail + k, b.len() - tail + k);
        out.push(DiffLine {
            op: '=',
            old: Some(oi + 1),
            new: Some(ni + 1),
            text: a[oi].to_string(),
        });
    }
    out
}

/// The changed lines with `context` unchanged lines around them, in hunks.
pub fn hunks(old: &str, new: &str, context: usize) -> Vec<Hunk> {
    let ops = line_ops(old, new);
    let changed: Vec<usize> = ops
        .iter()
        .enumerate()
        .filter(|(_, l)| l.op != '=')
        .map(|(i, _)| i)
        .collect();
    let mut out: Vec<Hunk> = Vec::new();
    let mut k = 0;
    while k < changed.len() {
        let from = changed[k].saturating_sub(context);
        let mut to = (changed[k] + context + 1).min(ops.len());
        k += 1;
        while k < changed.len() && changed[k] <= to + context {
            to = (changed[k] + context + 1).min(ops.len());
            k += 1;
        }
        let lines: Vec<DiffLine> = ops[from..to].to_vec();
        let old_start = lines
            .iter()
            .find_map(|l| l.old)
            .unwrap_or_else(|| prior(&ops[..from], |l| l.old));
        let new_start = lines
            .iter()
            .find_map(|l| l.new)
            .unwrap_or_else(|| prior(&ops[..from], |l| l.new));
        out.push(Hunk {
            old_start,
            old_len: lines.iter().filter(|l| l.op != '+').count(),
            new_start,
            new_len: lines.iter().filter(|l| l.op != '-').count(),
            lines,
        });
    }
    out
}

/// The line number before a hunk that holds none on one side.
fn prior(before: &[DiffLine], side: impl Fn(&DiffLine) -> Option<usize>) -> usize {
    before.iter().rev().find_map(side).unwrap_or(0)
}

/// Lines added and removed.
pub fn counts(old: &str, new: &str) -> (usize, usize) {
    line_ops(old, new)
        .iter()
        .fold((0, 0), |(a, r), l| match l.op {
            '+' => (a + 1, r),
            '-' => (a, r + 1),
            _ => (a, r),
        })
}

/// The diff as `git diff` prints it, for one file.
pub fn unified(path: &str, old: Option<&str>, new: Option<&str>) -> String {
    let mut s = format!(
        "--- {}\n+++ {}\n",
        if old.is_some() {
            format!("a/{path}")
        } else {
            "/dev/null".into()
        },
        if new.is_some() {
            format!("b/{path}")
        } else {
            "/dev/null".into()
        },
    );
    for h in hunks(old.unwrap_or(""), new.unwrap_or(""), 3) {
        s.push_str(&format!(
            "@@ -{},{} +{},{} @@\n",
            h.old_start, h.old_len, h.new_start, h.new_len
        ));
        for l in h.lines {
            s.push(if l.op == '=' { ' ' } else { l.op });
            s.push_str(&l.text);
            s.push('\n');
        }
    }
    s
}

/// fix-219: [`unified`], with its body (every line but the `--- `/`+++ `
/// file headers and the `@@ @@` hunk markers, which are structure rather
/// than change) capped at `max_lines`, then one "… N more lines" line. A
/// drift finding should show Kenny the actual old and new line, not just a
/// filename — and short enough to still read in place beside the finding,
/// which is what made the cap part of the ask rather than the whole diff.
pub fn unified_capped(
    path: &str,
    old: Option<&str>,
    new: Option<&str>,
    max_lines: usize,
) -> String {
    let full = unified(path, old, new);
    let is_header =
        |l: &str| l.starts_with("--- ") || l.starts_with("+++ ") || l.starts_with("@@ ");
    let body_total = full.lines().filter(|l| !is_header(l)).count();
    let mut out = String::new();
    let mut body_shown = 0usize;
    for line in full.lines() {
        if is_header(line) {
            out.push_str(line);
            out.push('\n');
            continue;
        }
        if body_shown >= max_lines {
            continue;
        }
        out.push_str(line);
        out.push('\n');
        body_shown += 1;
    }
    if body_total > max_lines {
        out.push_str(&format!("… {} more lines\n", body_total - max_lines));
    }
    out
}
