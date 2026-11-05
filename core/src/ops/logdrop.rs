//! `log_drop:` — lines of a stack's logs the shipper must not forward.
//!
//! Generic: the stack file lists the patterns, nothing here knows which app
//! produces them. Each entry is a regular expression in the syntax Alloy's
//! `stage.drop { expression = ... }` takes (RE2), matched UNANCHORED against
//! the log line, so a plain phrase works as a substring match. Rust's `regex`
//! crate is an RE2-family engine, so an entry it compiles is one Alloy loads;
//! that matters because Alloy refuses the whole config for one bad regex and
//! the stack would stop shipping altogether.

/// Alloy component every source forwards to when the stack drops anything.
const COMPONENT: &str = "drop";
const DROP_SINK: &str = "loki.process.drop.receiver";
const WRITE_SINK: &str = "loki.write.default.receiver";

/// Where a source's `forward_to` points: the drop stage when the stack
/// declares patterns, the writer (as before) when it does not.
pub fn sink(patterns: &[String]) -> &'static str {
    if patterns.is_empty() {
        WRITE_SINK
    } else {
        DROP_SINK
    }
}

/// The `loki.process "drop"` component, or nothing for an empty list so the
/// rendered config stays byte-identical to what it was before the key.
pub fn block(patterns: &[String]) -> String {
    if patterns.is_empty() {
        return String::new();
    }
    let stages: String = patterns
        .iter()
        .map(|p| {
            format!(
                "  stage.drop {{\n    expression = \"{}\"\n  }}\n",
                escape(p)
            )
        })
        .collect();
    format!(
        "\n// ── Lines this stack asked not to ship (`log_drop:`) ─────────────────────\n\
         loki.process \"{COMPONENT}\" {{\n{stages}  forward_to = [{WRITE_SINK}]\n}}\n"
    )
}

/// Alloy string literal escaping; validation already refused control chars.
fn escape(s: &str) -> String {
    s.replace('\\', "\\\\").replace('"', "\\\"")
}

/// Why an entry list cannot be rendered, one message per fault.
pub fn problems(patterns: &[String]) -> Vec<String> {
    let mut out = Vec::new();
    let mut seen: Vec<&str> = Vec::new();
    for p in patterns {
        if p.trim().is_empty() {
            out.push("log_drop has an empty entry :: it would drop every line; remove it".into());
        } else if p.chars().any(char::is_control) {
            out.push(format!(
                "log_drop entry {p:?} contains a newline or control character :: a line is \
                 matched one at a time; write the pattern for a single line"
            ));
        } else if let Err(e) = regex::Regex::new(p) {
            out.push(format!(
                "log_drop entry {p:?} is not a valid regular expression ({e}) :: Alloy would \
                 refuse the whole shipper config; escape special characters with a backslash"
            ));
        } else if seen.contains(&p.as_str()) {
            out.push(format!(
                "log_drop entry {p:?} is listed twice :: remove one"
            ));
        }
        seen.push(p);
    }
    out
}
