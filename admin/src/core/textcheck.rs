//! feat-kpadopt-2: where a YAML or TOML file stops reading, for the Files
//! editor's code field (kp-themes 11.2.0 draws the mark; it brings no
//! parser, so the dashboard answers with this one). Pure: text in, the first
//! problem out.

use serde::Serialize;
use serde_yaml::Value;

/// The kinds of file the code field checks.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Kind {
    Yaml,
    Toml,
}

/// Where and why a file does not read; line and column count from 1.
#[derive(Debug, Clone, PartialEq, Eq, Serialize)]
pub struct Problem {
    pub line: usize,
    pub column: usize,
    pub message: String,
}

/// The kind a file path is checked as, or none (any other file is plain text).
pub fn kind_of(path: &str) -> Option<Kind> {
    let lower = path.to_ascii_lowercase();
    if lower.ends_with(".yml") || lower.ends_with(".yaml") {
        Some(Kind::Yaml)
    } else if lower.ends_with(".toml") {
        Some(Kind::Toml)
    } else {
        None
    }
}

/// The first problem of `text`, or none when it reads.
pub fn check(kind: Kind, text: &str) -> Option<Problem> {
    match kind {
        Kind::Yaml => check_yaml(text),
        Kind::Toml => check_toml(text),
    }
}

fn check_yaml(text: &str) -> Option<Problem> {
    use serde::Deserialize;
    // Every document of the stream, as a compose file with `---` may hold.
    for document in serde_yaml::Deserializer::from_str(text) {
        if let Err(e) = Value::deserialize(document) {
            let (line, column) = e
                .location()
                .map(|l| (l.line(), l.column()))
                .unwrap_or((1, 1));
            let message = without_locations(&e.to_string());
            return Some(Problem {
                line,
                column,
                message,
            });
        }
    }
    None
}

/// The sentence without its " at line N column M" parts (the field names the
/// place itself): "did not find expected ',' at line 3 column 5, while parsing
/// a flow sequence at line 2 column 8" reads "did not find expected ',', while
/// parsing a flow sequence".
fn without_locations(said: &str) -> String {
    let mut out = String::new();
    let mut rest = said;
    while let Some(i) = rest.find(" at line ") {
        out.push_str(&rest[..i]);
        let after = &rest[i + " at line ".len()..];
        let digits = |t: &str| t.len() - t.trim_start_matches(|c: char| c.is_ascii_digit()).len();
        let line_end = digits(after);
        let tail = &after[line_end..];
        rest = match tail.strip_prefix(" column ") {
            Some(t) if line_end > 0 => &t[digits(t)..],
            _ => {
                // Not a place after all: the words stay.
                out.push_str(" at line ");
                after
            }
        };
    }
    out.push_str(rest);
    out
}

fn check_toml(text: &str) -> Option<Problem> {
    let e = text.parse::<toml::Table>().err()?;
    let at = e.span().map_or(0, |s| s.start).min(text.len());
    let before = text.get(..at).unwrap_or("");
    let line = before.matches('\n').count() + 1;
    let column = before
        .rsplit('\n')
        .next()
        .map_or(0, |last| last.chars().count())
        + 1;
    Some(Problem {
        line,
        column,
        message: e.message().to_string(),
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_file_is_checked_by_its_name() {
        assert_eq!(kind_of("lxc-compose.yml"), Some(Kind::Yaml));
        assert_eq!(kind_of("apps/web/Checks.YAML"), Some(Kind::Yaml));
        assert_eq!(kind_of("host.toml"), Some(Kind::Toml));
        assert_eq!(kind_of("README.md"), None);
        assert_eq!(kind_of("data.json"), None);
    }

    #[test]
    fn a_file_that_reads_has_no_problem() {
        assert_eq!(check(Kind::Yaml, "a: 1\nb:\n  - x\n"), None);
        assert_eq!(check(Kind::Yaml, "a: 1\n---\nb: 2\n"), None);
        assert_eq!(check(Kind::Yaml, ""), None);
        assert_eq!(check(Kind::Toml, "a = 1\n[t]\nb = \"x\"\n"), None);
        assert_eq!(check(Kind::Toml, ""), None);
    }

    #[test]
    fn a_yaml_problem_names_its_line_and_column() {
        let p = check(Kind::Yaml, "a: 1\nb: [1, 2\nc: 3\n").expect("a problem");
        assert!(p.line >= 2, "the flow list starts on line 2: {p:?}");
        assert!(p.column >= 1);
        assert!(!p.message.contains(" at line "), "{p:?}");
    }

    #[test]
    fn the_places_leave_the_sentence() {
        assert_eq!(
            without_locations(
                "did not find expected ',' at line 3 column 5, while parsing a flow sequence at line 2 column 8"
            ),
            "did not find expected ',', while parsing a flow sequence"
        );
        assert_eq!(without_locations("no place here"), "no place here");
        assert_eq!(without_locations("odd at line x"), "odd at line x");
    }

    #[test]
    fn a_toml_problem_names_its_line_and_column() {
        let p = check(Kind::Toml, "a = 1\nb = \n").expect("a problem");
        assert_eq!(p.line, 2);
        assert!(p.column >= 1);
        let p = check(Kind::Toml, "a = 1\nb 2\n").expect("a problem");
        assert_eq!(p.line, 2);
        assert_eq!(p.column, 3);
    }
}
