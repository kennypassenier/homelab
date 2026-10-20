//! ask-kp11be-6: a release's notes as the Update flow shows them. The text
//! comes from a third party (a GitHub release body), so it is treated as
//! untrusted: it is cleaned here, capped, and the page draws it only as text
//! (never as HTML). Pure: no I/O.

use serde::Serialize;

/// The most characters of notes the dashboard keeps and sends.
pub const MAX_CHARS: usize = 20_000;

/// One release's notes, clean.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Notes {
    pub repo: String,
    pub tag: String,
    /// The release's title ("v12.1.0"), cleaned like the body.
    pub name: String,
    pub body: String,
    /// The body was cut at [`MAX_CHARS`]; `total` is its full length.
    pub truncated: bool,
    pub total: usize,
    /// When GitHub says it was published (ISO 8601), as sent.
    pub published_at: Option<String>,
    /// The release page, rebuilt from the repo and tag (never GitHub's own
    /// `html_url`).
    pub url: String,
}

/// `owner/name`, the characters GitHub allows, nothing else.
pub fn valid_repo(s: &str) -> bool {
    let ok = |p: &str| {
        !p.is_empty()
            && p.len() <= 100
            && p.bytes()
                .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'-' | b'_' | b'.'))
            && p != "."
            && p != ".."
    };
    s.split_once('/')
        .is_some_and(|(o, n)| ok(o) && ok(n) && !n.contains('/'))
}

/// Make untrusted text safe to show as plain text: line ends normalised,
/// control characters dropped, HTML comments and tags removed (an autolink
/// `<https://…>` keeps its address), runs of blank lines folded. Returns the
/// text cut at `max` characters, whether it was cut, and the full length.
pub fn clean(text: &str, max: usize) -> (String, bool, usize) {
    let text = text.replace("\r\n", "\n").replace('\r', "\n");
    let mut out = String::with_capacity(text.len().min(max * 2));
    let chars: Vec<char> = text.chars().collect();
    let mut i = 0;
    while i < chars.len() {
        let c = chars[i];
        if c == '<' {
            // A comment goes whole.
            if chars[i..].starts_with(&['<', '!', '-', '-']) {
                let rest: String = chars[i..].iter().collect();
                if let Some(end) = rest.find("-->") {
                    i += rest[..end + 3].chars().count();
                    continue;
                }
            }
            // A tag, or an autolink: up to the next '>' on the same line.
            let next = chars.get(i + 1).copied().unwrap_or(' ');
            if next.is_ascii_alphabetic() || next == '/' || next == '!' || next == '?' {
                let close = chars[i + 1..]
                    .iter()
                    .take(300)
                    .position(|&x| x == '>' || x == '\n' || x == '<');
                if let Some(p) = close.filter(|&p| chars[i + 1 + p] == '>') {
                    let inner: String = chars[i + 1..i + 1 + p].iter().collect();
                    if inner.contains("://") && !inner.contains(char::is_whitespace) {
                        out.push_str(&inner);
                    }
                    i += p + 2;
                    continue;
                }
            }
        }
        if c == '\n' || c == '\t' || !c.is_control() {
            out.push(c);
        }
        i += 1;
    }
    // Fold three or more line breaks into one blank line; trim the ends.
    let mut folded = String::with_capacity(out.len());
    let mut breaks = 0;
    for c in out.chars() {
        if c == '\n' {
            breaks += 1;
            if breaks <= 2 {
                folded.push(c);
            }
        } else {
            breaks = 0;
            folded.push(c);
        }
    }
    let folded = folded.trim().to_string();
    let total = folded.chars().count();
    if total > max {
        (folded.chars().take(max).collect(), true, total)
    } else {
        (folded, false, total)
    }
}

/// GitHub's `releases/tags/{tag}` answer as clean notes.
pub fn from_github(json: &str, repo: &str, tag: &str) -> Result<Notes, String> {
    let v: serde_json::Value =
        serde_json::from_str(json).map_err(|e| format!("GitHub's release does not read: {e}"))?;
    let body = v["body"].as_str().unwrap_or("");
    let (body, truncated, total) = clean(body, MAX_CHARS);
    let (name, _, _) = clean(v["name"].as_str().unwrap_or(""), 200);
    Ok(Notes {
        repo: repo.to_string(),
        tag: tag.to_string(),
        name: name.lines().next().unwrap_or("").to_string(),
        body,
        truncated,
        total,
        published_at: v["published_at"]
            .as_str()
            .filter(|s| s.len() <= 40 && s.bytes().all(|b| b.is_ascii_graphic()))
            .map(str::to_string),
        url: format!("https://github.com/{repo}/releases/tag/{tag}"),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn markup_and_controls_never_reach_the_page() {
        let (t, cut, total) = clean(
            "## v2\r\n<script>alert(1)</script><img src=x onerror=y>\n<!-- hidden -->\x07ok\n\n\n\n<https://a.example/x>\n<b>bold</b>",
            MAX_CHARS,
        );
        assert!(!t.contains('<') && !t.contains('\x07') && !t.contains("hidden"));
        assert!(t.contains("alert(1)") && t.contains("https://a.example/x"));
        assert!(!t.contains("\n\n\n"));
        assert!(!cut && total == t.chars().count());
    }

    #[test]
    fn long_notes_are_cut_and_say_so() {
        let (t, cut, total) = clean(&"x".repeat(MAX_CHARS + 50), MAX_CHARS);
        assert!(cut && t.chars().count() == MAX_CHARS && total == MAX_CHARS + 50);
    }

    #[test]
    fn only_a_github_repo_name_is_taken() {
        assert!(valid_repo("immich-app/immich"));
        assert!(!valid_repo("a/b/c") && !valid_repo("../x") && !valid_repo("a/b?x=1"));
    }

    #[test]
    fn the_release_page_is_rebuilt_not_trusted() {
        let n = from_github(
            r#"{"name":"v1","body":"hi","html_url":"https://evil.example","published_at":"2026-10-01T10:00:00Z"}"#,
            "o/r",
            "v1",
        )
        .unwrap();
        assert_eq!(n.url, "https://github.com/o/r/releases/tag/v1");
    }
}
