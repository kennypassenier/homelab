//! A local `.env` that has fallen behind latch.
//!
//! A deploy reads `stacks/<stack>/<app>/.env` first and only asks latch when
//! the file is absent (a local file wins, decided with the `[env] <app> <-
//! local .env (latch skipped)` report). Nothing used to say when that local
//! copy no longer matched latch, so a stale working copy could put an old
//! value back. This module is the comparison: pure, names only. A value never
//! leaves this file, in a note, a log or a test failure message.

use std::collections::BTreeMap;

/// How a local env file differs from latch's copy of the same file.
#[derive(Debug, Default, PartialEq, Eq)]
pub struct EnvDrift {
    /// Keys present on both sides with different values.
    pub differ: Vec<String>,
    /// Keys only the local file has.
    pub only_local: Vec<String>,
    /// Keys only latch has.
    pub only_latch: Vec<String>,
}

impl EnvDrift {
    pub fn is_empty(&self) -> bool {
        self.differ.is_empty() && self.only_local.is_empty() && self.only_latch.is_empty()
    }
}

/// `KEY=VALUE` lines of a dotenv file: blank lines and `#` comments skipped,
/// an optional `export ` prefix dropped, one pair of surrounding quotes
/// removed. The last assignment of a key wins, as a shell would read it.
fn parse(text: &str) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    for line in text.lines() {
        let line = line.trim();
        if line.is_empty() || line.starts_with('#') {
            continue;
        }
        let line = line.strip_prefix("export ").unwrap_or(line);
        let Some((key, value)) = line.split_once('=') else {
            continue;
        };
        let value = value.trim();
        let value = ["\"", "'"]
            .iter()
            .find_map(|q| value.strip_prefix(q).and_then(|v| v.strip_suffix(q)))
            .unwrap_or(value);
        out.insert(key.trim().to_string(), value.to_string());
    }
    out
}

/// Compare the local file's text with latch's, key by key.
pub fn compare(local: &str, latch: &str) -> EnvDrift {
    let (local, latch) = (parse(local), parse(latch));
    let mut drift = EnvDrift::default();
    for (key, value) in &local {
        match latch.get(key) {
            Some(theirs) if theirs != value => drift.differ.push(key.clone()),
            Some(_) => {}
            None => drift.only_local.push(key.clone()),
        }
    }
    drift.only_latch = latch
        .keys()
        .filter(|k| !local.contains_key(*k))
        .cloned()
        .collect();
    drift
}

/// The deploy note for one app, `None` when the two files agree.
pub fn note(app: &str, drift: &EnvDrift) -> Option<String> {
    if drift.is_empty() {
        return None;
    }
    let mut parts = Vec::new();
    if !drift.differ.is_empty() {
        parts.push(format!("different value: {}", drift.differ.join(", ")));
    }
    if !drift.only_local.is_empty() {
        parts.push(format!(
            "only in the local file: {}",
            drift.only_local.join(", ")
        ));
    }
    if !drift.only_latch.is_empty() {
        parts.push(format!("only in latch: {}", drift.only_latch.join(", ")));
    }
    Some(format!(
        "[env] {app} :: the local .env differs from latch ({}) :: the local file is what this \
         deploy uses; `latch pull` brings it up to date, or delete it to use latch",
        parts.join("; ")
    ))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn equal_files_have_no_drift_whatever_the_formatting() {
        let local = "# c\nexport A=1\nB=\"two words\"\n\nC='x'\n";
        let latch = "A=1\nB=two words\nC=x\n";
        assert!(compare(local, latch).is_empty());
        assert_eq!(note("qbittorrent", &compare(local, latch)), None);
    }

    #[test]
    fn names_the_keys_on_each_side_and_never_a_value() {
        let local = "A=old-secret\nONLY_LOCAL=1\nSAME=s\n";
        let latch = "A=new-secret\nSAME=s\nONLY_LATCH=2\n";
        let drift = compare(local, latch);
        assert_eq!(drift.differ, ["A"]);
        assert_eq!(drift.only_local, ["ONLY_LOCAL"]);
        assert_eq!(drift.only_latch, ["ONLY_LATCH"]);
        let text = note("downloader", &drift).unwrap();
        assert!(text.contains("different value: A"));
        assert!(!text.contains("old-secret") && !text.contains("new-secret"));
    }

    #[test]
    fn the_last_assignment_wins_like_a_shell() {
        assert!(compare("A=1\nA=2\n", "A=2\n").is_empty());
    }
}
