//! Edit forms: the browser's field and rule checks, in the browser's words.

use super::*;

pub(super) fn text_of(v: Option<&Value>) -> String {
    match v {
        Some(Value::String(s)) => s.clone(),
        Some(Value::Null) | None => String::new(),
        Some(Value::Bool(b)) => b.to_string(),
        Some(other) => other.to_string(),
    }
}

pub(super) fn all_digits(t: &str) -> bool {
    !t.is_empty() && t.chars().all(|c| c.is_ascii_digit())
}

pub(super) fn matches(pattern: &str, text: &str) -> bool {
    regex::Regex::new(&format!("^(?:{pattern})$"))
        .map(|r| r.is_match(text))
        .unwrap_or(false)
}

/// What is wrong with one non-empty value of a number, typed or patterned
/// field.
fn value_problem(f: &Field, text: &str) -> Option<String> {
    if f.kind == FieldKind::Number {
        let (min, max) = (f.min.unwrap_or(0), f.max.unwrap_or(i64::MAX));
        let ok = all_digits(text) && text.parse::<i64>().is_ok_and(|n| (min..=max).contains(&n));
        let (lo, hi) = (min.to_string(), max.to_string());
        (!ok).then(|| say("number", &[("label", &f.label), ("min", &lo), ("max", &hi)]))
    } else if f.kind == FieldKind::Typed && Some(&text.to_string()) != f.expect.as_ref() {
        Some(say(
            "typed",
            &[("expect", f.expect.as_deref().unwrap_or(""))],
        ))
    } else if f.pattern.as_deref().is_some_and(|p| !matches(p, text)) {
        let lower = f.label.to_lowercase();
        Some(say("pattern", &[("text", text), ("lower", &lower)]))
    } else {
        None
    }
}

/// `checkFields`: what is wrong with the values, by field name.
pub fn check_fields<'a>(
    fields: impl IntoIterator<Item = &'a Field>,
    values: &Values,
) -> BTreeMap<String, String> {
    let mut errors = BTreeMap::new();
    for f in fields {
        if f.kind == FieldKind::Check {
            continue;
        }
        let text = match values.get(&f.name) {
            Some(Value::String(s)) => s.trim().to_string(),
            _ => String::new(),
        };
        if text.is_empty() {
            if f.required {
                errors.insert(f.name.clone(), say("needed", &[("label", &f.label)]));
            }
        } else if let Some(why) = value_problem(f, &text) {
            errors.insert(f.name.clone(), why);
        }
    }
    errors
}

/// What is wrong with a peer address or network, if anything.
fn peer_problem(peer: &str) -> Option<String> {
    let re = regex::Regex::new(r"^(\d{1,3})\.(\d{1,3})\.(\d{1,3})\.(\d{1,3})(?:/(\d{1,2}))?$")
        .expect("the address pattern reads");
    let words = |key: &str| Some(say(key, &[("peer", peer)]));
    match re.captures(peer) {
        Some(c) if (1..=4).all(|i| c[i].parse::<u32>().unwrap_or(999) <= 255) => {
            let bits: u32 = c.get(5)?.as_str().parse().unwrap_or(99);
            if bits > 32 {
                return words("peer_prefix");
            }
            let o = |i: usize| c[i].parse::<u32>().unwrap_or(0);
            let ip = (o(1) << 24) | (o(2) << 16) | (o(3) << 8) | o(4);
            let mask = if bits == 0 {
                0
            } else {
                u32::MAX << (32 - bits)
            };
            if ip & mask != ip {
                return words("peer_host_bits");
            }
            None
        }
        _ => words("peer_form"),
    }
}

/// A port or `from:to` range as Proxmox reads it; an empty end is 0.
fn port_number(s: Option<&str>) -> Option<f64> {
    let s = s?;
    if s.is_empty() {
        Some(0.0)
    } else {
        s.parse::<f64>().ok().filter(|n| n.is_finite())
    }
}

/// The first comma-separated part that is not a port or a port range.
fn bad_port_part(ports: &str) -> Option<&str> {
    let ok = |n: Option<f64>| n.is_some_and(|n| n.fract() == 0.0 && (1.0..=65535.0).contains(&n));
    ports.split(',').find(|part| {
        let mut it = part.split(':');
        let a = port_number(it.next());
        let b = port_number(it.next());
        !ok(a) || (part.contains(':') && (!ok(b) || b <= a))
    })
}

/// What is wrong with the ports for this protocol, if anything.
fn port_problem(ports: &str, proto: &str) -> Option<String> {
    if proto.is_empty() || proto == "icmp" {
        let key = if proto == "icmp" {
            "icmp_ports"
        } else {
            "port_proto"
        };
        return Some(say(key, &[]));
    }
    bad_port_part(ports).map(|part| say("port_range", &[("part", part)]))
}

/// `ruleProblems`: what Proxmox would reject or misread, by field.
pub fn rule_problems(v: &Values) -> BTreeMap<String, String> {
    let mut out = BTreeMap::new();
    let peer = text_of(v.get("peer")).trim().to_string();
    if !peer.is_empty()
        && let Some(why) = peer_problem(&peer)
    {
        out.insert("peer".into(), why);
    }
    let ports: String = text_of(v.get("dport"))
        .chars()
        .filter(|c| !c.is_whitespace())
        .collect();
    if !ports.is_empty()
        && let Some(why) = port_problem(&ports, &text_of(v.get("proto")))
    {
        out.insert("dport".into(), why);
    }
    if text_of(v.get("note")).contains('\n') {
        out.insert("note".into(), say("note_line", &[]));
    }
    out
}
