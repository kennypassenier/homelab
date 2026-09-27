//! fix-32: a credential never goes into a command's argv.
//!
//! `/proc/<pid>/cmdline` is readable by every user on the machine while
//! `/proc/<pid>/environ` is not, so `curl -u "$U:$P"` shows the Grafana admin
//! password to anything that lists processes for as long as curl runs. The
//! architecture reference promised "secrets never enter argv" while three
//! commands did exactly that; the device backup had it right all along with
//! `curl -K`. This scans every script the orchestrator ships for the shape.

use std::path::{Path, PathBuf};

fn root() -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .to_path_buf()
}

fn walk(dir: &Path, out: &mut Vec<PathBuf>) {
    let Ok(entries) = std::fs::read_dir(dir) else {
        return;
    };
    for e in entries.flatten() {
        let p = e.path();
        if p.is_dir() {
            walk(&p, out);
        } else if p
            .extension()
            .is_some_and(|x| x == "rs" || x == "yml" || x == "yaml" || x == "sh")
        {
            out.push(p);
        }
    }
}

/// `curl … -u "$VAR…"` / `--user "$VAR…"`: a credential expanded into argv.
fn credential_in_curl_argv(line: &str) -> bool {
    let code = line.trim_start();
    if code.starts_with("//") || code.starts_with('#') {
        return false;
    }
    let Some(at) = line.find("curl ") else {
        return false;
    };
    let rest = &line[at..];
    ["-u \"$", "-u \\\"$", "--user \"$", "--user \\\"$", "-u $"]
        .iter()
        .any(|p| rest.contains(p))
}

#[test]
fn fix_32_no_shipped_script_puts_a_credential_in_curl_argv() {
    let mut files = Vec::new();
    for dir in ["core/src", "host/src", "client/src", "stacks", "presets"] {
        walk(&root().join(dir), &mut files);
    }
    assert!(files.len() > 50, "only {} files scanned", files.len());
    let mut hits = Vec::new();
    for f in files {
        let text = std::fs::read_to_string(&f).unwrap_or_default();
        for (i, line) in text.lines().enumerate() {
            if credential_in_curl_argv(line) {
                let rel = f.strip_prefix(root()).unwrap().display().to_string();
                hits.push(format!("{rel}:{}", i + 1));
            }
        }
    }
    assert!(
        hits.is_empty(),
        "credential in curl argv — feed it with `-K -` from printf instead:\n{}",
        hits.join("\n")
    );
}

#[test]
fn fix_32_the_detector_sees_the_shape_and_ignores_comments() {
    assert!(credential_in_curl_argv(r#"curl -s -u "$U:$P" http://x"#));
    assert!(credential_in_curl_argv(
        r#"  curl -s -m 15 -u \"$U:$P\" 'http://x'"#
    ));
    assert!(!credential_in_curl_argv(
        r#"/// Deliberate: `-u "$(cat file)"` would put it in curl's argv"#
    ));
    assert!(!credential_in_curl_argv(
        r#"printf 'user = "%s:%s"\n' "$U" "$P" | curl -s -K - http://x"#
    ));
    assert!(!credential_in_curl_argv(r#"journalctl -u "$UNIT" -n 6"#));
}
