//! ask-kp11be-9: request samples of one hostname from the proxy's access
//! log, for the Metrics Traffic tab's "recent requests" table.
//!
//! The privacy rule (Kenny, 2026-09-17: what a person browses is not his
//! business) is kept here, at the point where a log line becomes a row:
//! a sample carries the method, the path, the status and the duration and
//! nothing else. The client address, the user agent, the referrer and the
//! query string never leave this function; they are not read, not "read
//! and hidden". Pure: no I/O.

use serde::Serialize;

use super::logs::LogLine;

/// How many samples one read returns by default and at most.
pub const DEFAULT_SAMPLES: usize = 50;
pub const MAX_SAMPLES: usize = 200;
/// The longest path kept; the rest is cut (a scanner's path can be long).
const MAX_PATH: usize = 200;

/// One request, as the table shows it.
#[derive(Debug, Clone, PartialEq, Serialize)]
pub struct Sample {
    /// Unix milliseconds; the table is ordered by it, newest first.
    pub at_ms: u64,
    pub method: String,
    /// The path without its query string.
    pub path: String,
    pub status: u16,
    /// How long the proxy took to answer, milliseconds; None when the log
    /// line has no duration.
    pub duration_ms: Option<f64>,
}

/// A hostname that is safe to put inside a LogQL string: letters, digits,
/// dots, hyphens and underscores, as a DNS name has them. Anything else is
/// refused before a query is built.
pub fn valid_host(host: &str) -> bool {
    !host.is_empty()
        && host.len() <= 253
        && host
            .bytes()
            .all(|b| b.is_ascii_alphanumeric() || matches!(b, b'.' | b'-' | b'_'))
}

/// The path of a request line: everything before `?` or `#`, cut to
/// [`MAX_PATH`] characters.
fn clean_path(raw: &str) -> String {
    let end = raw.find(['?', '#']).unwrap_or(raw.len());
    raw[..end].chars().take(MAX_PATH).collect()
}

/// One access-log line (Traefik's JSON) as a sample; None for a line that
/// is not JSON or has no status.
pub fn parse_sample(at_ms: u64, line: &str) -> Option<Sample> {
    let v: serde_json::Value = serde_json::from_str(line).ok()?;
    let text = |k: &str| v.get(k).and_then(|x| x.as_str());
    let status = v
        .get("DownstreamStatus")
        .and_then(|s| s.as_u64().or_else(|| s.as_str()?.parse().ok()))
        .and_then(|s| u16::try_from(s).ok())?;
    let method: String = text("RequestMethod")
        .unwrap_or("")
        .chars()
        .filter(|c| c.is_ascii_alphabetic())
        .take(12)
        .collect();
    // The proxy logs the duration in nanoseconds.
    let duration_ms = v
        .get("Duration")
        .and_then(|d| d.as_f64())
        .map(|ns| (ns / 100_000.0).round() / 10.0);
    Some(Sample {
        at_ms,
        method,
        path: clean_path(text("RequestPath").unwrap_or("")),
        status,
        duration_ms,
    })
}

/// The samples of a set of lines, newest first, at most `limit`.
pub fn samples(lines: &[LogLine], limit: usize) -> Vec<Sample> {
    let mut out: Vec<Sample> = lines
        .iter()
        .filter_map(|l| parse_sample(l.ts_ms, &l.line))
        .collect();
    out.sort_by_key(|s| std::cmp::Reverse(s.at_ms));
    out.truncate(limit.min(MAX_SAMPLES));
    out
}

#[cfg(test)]
mod tests {
    use super::*;

    const LINE: &str = r#"{"ClientHost":"192.168.1.23","ClientAddr":"192.168.1.23:5511","RequestHost":"films.example.org","RequestMethod":"GET","RequestPath":"/Items/abc/Images/Primary?api_key=SECRET&x=1","RequestProtocol":"HTTP/2.0","DownstreamStatus":200,"Duration":12345678,"request_User-Agent":"Mozilla"}"#;

    #[test]
    fn a_sample_carries_four_things_and_never_the_client() {
        let s = parse_sample(1_000, LINE).unwrap();
        assert_eq!(
            (s.method.as_str(), s.path.as_str(), s.status),
            ("GET", "/Items/abc/Images/Primary", 200)
        );
        assert_eq!(s.duration_ms, Some(12.3));
        let json = serde_json::to_string(&s).unwrap();
        for leak in ["192.168", "SECRET", "Mozilla", "api_key", "ClientHost"] {
            assert!(!json.contains(leak), "{leak} in {json}");
        }
    }

    #[test]
    fn a_line_without_a_status_or_json_is_no_sample() {
        assert!(parse_sample(1, "not json").is_none());
        assert!(parse_sample(1, r#"{"RequestPath":"/"}"#).is_none());
        // A status that is a string is read too.
        assert_eq!(
            parse_sample(1, r#"{"DownstreamStatus":"404","RequestPath":"/x"}"#).map(|s| s.status),
            Some(404)
        );
    }

    #[test]
    fn a_hostname_cannot_carry_a_query() {
        assert!(valid_host("films.example.org"));
        assert!(!valid_host(""));
        assert!(!valid_host("a\"} |= \"b"));
        assert!(!valid_host("a b"));
    }

    #[test]
    fn samples_are_newest_first_and_capped() {
        let mk = |ts| LogLine {
            ts_ms: ts,
            source: String::new(),
            stream: String::new(),
            level: String::new(),
            line: LINE.to_string(),
        };
        let lines: Vec<LogLine> = (1..=5).map(mk).collect();
        let got = samples(&lines, 3);
        assert_eq!(got.iter().map(|s| s.at_ms).collect::<Vec<_>>(), [5, 4, 3]);
    }

    #[test]
    fn a_long_path_is_cut() {
        let long = format!("/{}", "a".repeat(500));
        assert_eq!(clean_path(&long).chars().count(), MAX_PATH);
    }
}
